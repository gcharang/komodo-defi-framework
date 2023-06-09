use super::{BlockDbError, BlockDbImpl};

use async_trait::async_trait;
use mm2_core::mm_ctx::MmArc;
use mm2_db::indexed_db::{BeBigUint, DbIdentifier, DbInstance, DbUpgrader, IndexedDb, IndexedDbBuilder, InitDbResult,
                         OnUpgradeResult, TableSignature};
use mm2_db::indexed_db::{ConstructibleDb, DbLocked};
use mm2_err_handle::prelude::*;
use num_traits::ToPrimitive;
use std::path::Path;
use zcash_client_backend::data_api::BlockSource;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_primitives::consensus::BlockHeight;

const DB_NAME: &str = "z_compactblocks_cache";
const DB_VERSION: u32 = 1;

pub type BlockDbRes<T> = MmResult<T, BlockDbError>;
pub type BlockDbInnerLocked<'a> = DbLocked<'a, BlockDbInner>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockDbTable {
    height: BeBigUint,
    data: Vec<u8>,
    ticker: String,
}

impl BlockDbTable {
    pub const TICKER_HEIGHT_INDEX: &str = "block_height_ticker_index";
}

impl TableSignature for BlockDbTable {
    fn table_name() -> &'static str { "compactblocks" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_HEIGHT_INDEX, &["ticker", "height"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

pub struct BlockDbInner {
    pub inner: IndexedDb,
}

#[async_trait]
impl DbInstance for BlockDbInner {
    fn db_name() -> &'static str { DB_NAME }

    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        let inner = IndexedDbBuilder::new(db_id)
            .with_version(DB_VERSION)
            .with_table::<BlockDbTable>()
            .build()
            .await?;

        Ok(Self { inner })
    }
}

impl BlockDbInner {
    pub fn get_inner(&self) -> &IndexedDb { &self.inner }
}

impl BlockDbImpl {
    pub async fn new(ctx: MmArc, ticker: String, _path: impl AsRef<Path>) -> Result<Self, BlockDbError> {
        Ok(Self {
            db: ConstructibleDb::new(&ctx).into_shared(),
            ticker,
        })
    }

    #[allow(unused)]
    async fn lock_db(&self) -> BlockDbRes<BlockDbInnerLocked<'_>> {
        self.db
            .get_or_initialize()
            .await
            .mm_err(|err| BlockDbError::init_err(&self.ticker, err.to_string()))
    }

    pub async fn get_latest_block(&self) -> Result<u32, BlockDbError> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| BlockDbError::get_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| BlockDbError::get_err(&ticker, err.to_string()))?;
        let block_db = db_transaction
            .table::<BlockDbTable>()
            .await
            .map_err(|err| BlockDbError::table_err(&ticker, err.to_string()))?;
        let maybe_height = block_db
            .cursor_builder()
            .only("ticker", ticker.clone())
            .map_err(|err| BlockDbError::get_err(&ticker, err.to_string()))?
            .bound("height", BeBigUint::from(0u32), BeBigUint::from(u32::MAX))
            .reverse()
            .open_cursor(BlockDbTable::TICKER_HEIGHT_INDEX)
            .await
            .map_err(|err| BlockDbError::get_err(&ticker, err.to_string()))?
            .next()
            .await
            .map_err(|err| BlockDbError::get_err(&ticker, err.to_string()))?;

        let maybe_height = maybe_height
            .map(|(_, item)| {
                item.height
                    .to_u32()
                    .ok_or_else(|| BlockDbError::get_err(&ticker, "height is too large".to_string()))
            })
            .transpose()?;

        let Some(height) = maybe_height else {
            return Err(BlockDbError::not_found(&ticker, format!("block height not found")));
        };

        Ok(height)
    }

    pub async fn insert_block(&self, _height: u32, _cb_bytes: Vec<u8>) -> Result<usize, BlockDbError> { todo!() }

    pub async fn rewind_to_height(&self, _height: u32) -> Result<usize, BlockDbError> { todo!() }

    pub async fn with_blocks<F>(
        &self,
        _from_height: BlockHeight,
        _limit: Option<u32>,
        mut _with_row: F,
    ) -> Result<(), BlockDbError>
    where
        F: FnMut(CompactBlock) -> Result<(), BlockDbError>,
    {
        todo!()
    }
}

impl BlockSource for BlockDbImpl {
    type Error = BlockDbError;
    fn with_blocks<F>(&self, _from_height: BlockHeight, _limit: Option<u32>, _with_row: F) -> Result<(), Self::Error>
    where
        F: FnMut(CompactBlock) -> Result<(), Self::Error>,
    {
        todo!()
    }
}
