use crate::z_coin::z_coin_errors::ZcoinStorageError;

use mm2_core::mm_ctx::MmArc;
use mm2_db::indexed_db::{ConstructibleDb, DbIdentifier, DbInstance, DbLocked, DbUpgrader, IndexedDb, IndexedDbBuilder,
                         InitDbResult, OnUpgradeResult, SharedDb, TableSignature};
use mm2_err_handle::prelude::*;

const DB_NAME: &str = "z_params";
const DB_VERSION: u32 = 1;

pub type ZcashParamsWasmRes<T> = MmResult<T, ZcoinStorageError>;
pub type ZcashParamsInnerLocked<'a> = DbLocked<'a, ZcashParamsWasmInner>;

//  indexeddb max data =267386880 bytes to save, so we need to split sapling_spend
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ZcashParamsWasmTable {
    sapling_spend_id: u8,
    sapling_spend: Vec<u8>,
    sapling_output: Vec<u8>,
    ticker: String,
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
            table.create_index("ticker", false)?;
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

        let sapling_spend_chunks = sapling_spend_to_chunks(sapling_spend);
        for i in 0..12 {
            let sapling_output = if i > 0 { vec![] } else { sapling_output.to_vec() };
            let params = ZcashParamsWasmTable {
                sapling_spend_id: i as u8,
                sapling_spend: sapling_spend_chunks[i].clone(),
                sapling_output,
                ticker: "z_params".to_string(),
            };
            params_db.add_item(&params).await?;
        }

        Ok(())
    }

    pub async fn check_params(&self) -> MmResult<bool, ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let params_db = db_transaction.table::<ZcashParamsWasmTable>().await?;

        let maybe_param = params_db
            .cursor_builder()
            .only("ticker", "z_params")?
            .open_cursor("ticker")
            .await?
            .next()
            .await?;

        Ok(maybe_param.is_some())
    }

    pub async fn get_params(&self) -> MmResult<(Vec<u8>, Vec<u8>), ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let params_db = db_transaction.table::<ZcashParamsWasmTable>().await?;
        let mut maybe_params = params_db
            .cursor_builder()
            .only("ticker", "z_params")?
            .open_cursor("ticker")
            .await?;

        let mut sapling_spend = vec![];
        let mut sapling_output = vec![];

        while let Some((_, params)) = maybe_params.next().await? {
            sapling_spend.extend_from_slice(&params.sapling_spend);
            if params.sapling_spend_id == 0 {
                sapling_output = params.sapling_output
            }
        }

        Ok((sapling_spend, sapling_output.clone()))
    }
}

fn sapling_spend_to_chunks(sapling_spend: &[u8]) -> Vec<Vec<u8>> {
    // Set the target chunk size
    let target_chunk_size = 12;
    // Calculate the target size for each chunk
    let chunk_size = sapling_spend.len() / target_chunk_size;
    // Calculate the remainder for cases when the length is not perfectly divisible
    let remainder = sapling_spend.len() % target_chunk_size;
    let mut sapling_spend_chunks: Vec<Vec<u8>> = Vec::with_capacity(target_chunk_size);
    let mut start = 0;
    for i in 0..target_chunk_size {
        let end = start + chunk_size + if i < remainder { 1 } else { 0 };
        // Extract the current chunk from the original vector
        sapling_spend_chunks.push(sapling_spend[start..end].to_vec());
        // Move the start index to the next position
        start = end;
    }

    sapling_spend_chunks
}
