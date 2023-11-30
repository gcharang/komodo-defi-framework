use crypto::EncryptedMnemonicData;
use mm2_core::mm_ctx::MmArc;
use mm2_core::DbNamespaceId;
use mm2_db::indexed_db::{DbIdentifier, DbInstance, DbUpgrader, IndexedDb, IndexedDbBuilder, OnUpgradeError,
                         OnUpgradeResult, TableSignature};
use mm2_err_handle::prelude::*;
use std::collections::HashMap;

type WalletsDBResult<T> = Result<T, MmError<WalletsDBError>>;

#[derive(Debug, Deserialize, Display, Serialize)]
pub enum WalletsDBError {
    #[display(fmt = "Error deserializing '{}': {}", field, error)]
    DeserializationError {
        field: String,
        error: String,
    },
    #[display(fmt = "Error serializing '{}': {}", field, error)]
    SerializationError {
        field: String,
        error: String,
    },
    Internal(String),
}

#[derive(Debug, Deserialize, Serialize)]
struct MnemonicsTable {
    wallet_name: String,
    encrypted_mnemonic: String,
}

impl TableSignature for MnemonicsTable {
    fn table_name() -> &'static str { "mnemonics" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, mut old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        while old_version < new_version {
            match old_version {
                0 => {
                    // do nothing explicitly because the table should be created on upgrade
                    // from version 1 to 2 in order to avoid breaking existing databases
                },
                1 => {
                    let table = upgrader.create_table(Self::table_name())?;
                    table.create_index("wallet_name", true)?;
                },
                unsupported_version => {
                    return MmError::err(OnUpgradeError::UnsupportedVersion {
                        unsupported_version,
                        old_version,
                        new_version,
                    })
                },
            }

            old_version += 1;
        }
        Ok(())
    }
}

pub(super) async fn save_encrypted_passphrase(
    ctx: &MmArc,
    wallet_name: &str,
    encrypted_passphrase_data: &EncryptedMnemonicData,
) -> WalletsDBResult<()> {
    const DB_VERSION: u32 = 1;

    // Create the database identifier
    let db_name = "wallets";
    let db_id = match ctx.db_namespace {
        DbNamespaceId::Main => format!("MAIN::KOMODEFI::{}", db_name),
        DbNamespaceId::Test(u) => format!("TEST_{}::KOMODEFI::{}", u, db_name),
    };

    let indexed_db_builder = IndexedDbBuilder {
        db_name: db_id,
        db_version: 1,
        tables: HashMap::new(),
    };

    // Initialize the database instance with the mnemonic table
    let db = indexed_db_builder
        .with_version(DB_VERSION)
        .with_table::<MnemonicsTable>()
        .build()
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;

    let transaction = db
        .transaction()
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;
    let table = transaction
        .table::<MnemonicsTable>()
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;

    let mnemonics_table_item = MnemonicsTable {
        wallet_name: wallet_name.to_string(),
        encrypted_mnemonic: serde_json::to_string(encrypted_passphrase_data).map_err(|e| {
            WalletsDBError::SerializationError {
                field: "encrypted_mnemonic".to_string(),
                error: e.to_string(),
            }
        })?,
    };
    table
        .add_item(&mnemonics_table_item)
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;

    Ok(())
}

pub(super) async fn read_encrypted_passphrase(
    ctx: &MmArc,
    wallet_name: &str,
) -> WalletsDBResult<Option<EncryptedMnemonicData>> {
    const DB_VERSION: u32 = 1;

    // Create the database identifier
    let db_name = "wallets";
    let db_id = match ctx.db_namespace {
        DbNamespaceId::Main => format!("MAIN::KOMODEFI::{}", db_name),
        DbNamespaceId::Test(u) => format!("TEST_{}::KOMODEFI::{}", u, db_name),
    };

    let indexed_db_builder = IndexedDbBuilder {
        db_name: db_id,
        db_version: 1,
        tables: HashMap::new(),
    };

    // Initialize the database instance with the mnemonic table
    let db = indexed_db_builder
        .with_version(DB_VERSION)
        .with_table::<MnemonicsTable>()
        .build()
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;

    let transaction = db
        .transaction()
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;
    let table = transaction
        .table::<MnemonicsTable>()
        .await
        .mm_err(|e| WalletsDBError::Internal(e.to_string()))?;

    match table.get_item_by_unique_index("wallet_name", wallet_name).await {
        Ok(Some((_item_id, wallet_table_item))) => serde_json::from_str(&wallet_table_item.encrypted_mnemonic)
            .map_to_mm(|e| WalletsDBError::DeserializationError {
                field: "encrypted_mnemonic".to_string(),
                error: e.to_string(),
            }),
        Ok(None) => Ok(None),
        Err(e) => MmError::err(WalletsDBError::Internal(format!(
            "Error retrieving encrypted passphrase: {}",
            e
        ))),
    }
}
