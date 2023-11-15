use crate::z_coin::z_coin_errors::ZcoinStorageError;

use mm2_core::mm_ctx::MmArc;
use mm2_db::indexed_db::{ConstructibleDb, DbIdentifier, DbInstance, DbLocked, DbUpgrader, IndexedDb, IndexedDbBuilder,
                         InitDbResult, OnUpgradeResult, SharedDb, TableSignature};
use mm2_err_handle::prelude::*;

const DB_NAME: &str = "z_params";
const DB_VERSION: u32 = 1;

pub type ZcashParamsWasmRes<T> = MmResult<T, ZcoinStorageError>;
pub type ZcashParamsInnerLocked<'a> = DbLocked<'a, ZcashParamsWasmInner>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ZcashParamsWasmTable {
    sapling_spend1: Vec<u8>,
    sapling_spend2: Vec<u8>,
    sapling_spend3: Vec<u8>,
    sapling_spend4: Vec<u8>,
    sapling_spend5: Vec<u8>,
    sapling_output: Vec<u8>,
}

impl ZcashParamsWasmTable {
    pub const SPEND_OUTPUT_INDEX: &str = "sapling_spend_sapling_output_index";
}

impl TableSignature for ZcashParamsWasmTable {
    const TABLE_NAME: &'static str = "z_params_bytes";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
            table.create_multi_index(Self::SPEND_OUTPUT_INDEX, &["sapling_spend", "sapling_output"], true)?;
            table.create_index("sapling_spend", false)?;
            table.create_index("sapling_output", false)?;
        }

        Ok(())
    }
}

pub struct ZcashParamsWasmInner(IndexedDb);

#[async_trait::async_trait]
impl DbInstance for ZcashParamsWasmInner {
    const DB_NAME: &'static str = DB_NAME;

    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        let inner = IndexedDbBuilder::new(db_id)
            .with_version(DB_VERSION)
            .with_table::<ZcashParamsWasmTable>()
            .build()
            .await?;

        Ok(Self(inner))
    }
}

impl ZcashParamsWasmInner {
    pub fn get_inner(&self) -> &IndexedDb { &self.0 }
}

#[derive(Clone)]
pub struct ZcashParamsWasmImpl(SharedDb<ZcashParamsWasmInner>);

impl ZcashParamsWasmImpl {
    pub async fn new(ctx: MmArc) -> MmResult<Self, ZcoinStorageError> {
        Ok(Self(ConstructibleDb::new(&ctx).into_shared()))
    }

    async fn lock_db(&self) -> ZcashParamsWasmRes<ZcashParamsInnerLocked<'_>> {
        self.0
            .get_or_initialize()
            .await
            .mm_err(|err| ZcoinStorageError::DbError(err.to_string()))
    }

    pub async fn save_params(&self, sapling_spend: &[u8], sapling_output: &[u8]) -> MmResult<(), ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let params_db = db_transaction.table::<ZcashParamsWasmTable>().await?;

        let mut sapling_spend_chunks: Vec<Vec<u8>> = sapling_spend.chunks(5).map(|chunk| chunk.to_vec()).collect();
        // Ensure we have at least 5 chunks
        while sapling_spend_chunks.len() < 5 {
            sapling_spend_chunks.push(Vec::new());
        }

        let params = ZcashParamsWasmTable {
            sapling_spend1: sapling_spend_chunks[0].clone(),
            sapling_spend2: sapling_spend_chunks[1].clone(),
            sapling_spend3: sapling_spend_chunks[2].clone(),
            sapling_spend4: sapling_spend_chunks[3].clone(),
            sapling_spend5: sapling_spend_chunks[4].clone(),
            sapling_output: sapling_output.to_vec(),
        };
        params_db.add_item(&params).await?;

        Ok(())
    }

    pub async fn check_params(&self) -> MmResult<bool, ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let params_db = db_transaction.table::<ZcashParamsWasmTable>().await?;

        let maybe_param = params_db
            .cursor_builder()
            .open_cursor(ZcashParamsWasmTable::SPEND_OUTPUT_INDEX)
            .await?
            .next()
            .await?;

        Ok(maybe_param.is_some())
    }

    pub async fn get_params(&self) -> MmResult<(Vec<u8>, Vec<u8>), ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let params_db = db_transaction.table::<ZcashParamsWasmTable>().await?;

        let maybe_params = params_db.get_all_items().await?;
        match maybe_params.first() {
            Some((_, p)) => {
                let mut sapling_spend = Vec::with_capacity(
                    p.sapling_spend1.len()
                        + p.sapling_spend2.len()
                        + p.sapling_spend3.len()
                        + p.sapling_spend4.len()
                        + p.sapling_spend5.len(),
                );

                for chunk in [
                    &p.sapling_spend1,
                    &p.sapling_spend2,
                    &p.sapling_spend3,
                    &p.sapling_spend4,
                    &p.sapling_spend5,
                ] {
                    sapling_spend.extend_from_slice(chunk);
                }

                Ok((sapling_spend, p.sapling_output.clone()))
            },
            None => MmError::err(ZcoinStorageError::CorruptedData(
                "No z_cash params found in storage".to_string(),
            )),
        }
    }
}
