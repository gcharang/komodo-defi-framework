use crate::z_coin::storage::CompactBlockRow;
use crate::z_coin::storage::{BlockDbError, BlockDbImpl, BlockProcessingMode, ZcoinConsensusParams};

use async_trait::async_trait;
use mm2_core::mm_ctx::MmArc;
use mm2_db::indexed_db::{BeBigUint, DbIdentifier, DbInstance, DbUpgrader, IndexedDb, IndexedDbBuilder, InitDbResult,
                         MultiIndex, OnUpgradeResult, TableSignature};
use mm2_db::indexed_db::{ConstructibleDb, DbLocked};
use mm2_err_handle::prelude::*;
use num_traits::ToPrimitive;
use std::path::Path;
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;

const DB_NAME: &str = "z_compactblocks_cache";
const DB_VERSION: u32 = 1;

pub type BlockDbRes<T> = MmResult<T, BlockDbError>;
pub type BlockDbInnerLocked<'a> = DbLocked<'a, BlockDbInner>;

#[cfg(target_arch = "wasm32")]
impl BlockDbError {
    pub(crate) fn add_err(ticker: &str, err: String, height: u32) -> Self {
        Self::AddToStorageErr {
            ticker: ticker.to_string(),
            err,
            height,
        }
    }

    pub(crate) fn get_err(ticker: &str, err: String) -> Self {
        Self::GetFromStorageError {
            ticker: ticker.to_string(),
            err,
        }
    }

    pub(crate) fn remove_err(ticker: &str, err: String, height: u32) -> Self {
        Self::RemoveFromStorageErr {
            ticker: ticker.to_string(),
            err,
            height,
        }
    }

    pub(crate) fn init_err(ticker: &str, err: String) -> Self {
        Self::InitDbError {
            ticker: ticker.to_string(),
            err,
        }
    }

    pub(crate) fn not_found(ticker: &str, err: String) -> Self {
        Self::BlockHeightNotFound {
            ticker: ticker.to_string(),
            err,
        }
    }

    pub(crate) fn table_err(ticker: &str, err: String) -> Self {
        Self::IdbTableError {
            ticker: ticker.to_string(),
            err,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockDbTable {
    height: u32,
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
            table.create_index("height", false)?;
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
    pub async fn new(ctx: MmArc, ticker: String, _path: Option<impl AsRef<Path>>) -> Result<Self, BlockDbError> {
        Ok(Self {
            db: ConstructibleDb::new(&ctx).into_shared(),
            ticker,
        })
    }

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
            .bound("height", 0u32, u32::MAX)
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

    pub async fn insert_block(&self, height: u32, cb_bytes: Vec<u8>) -> Result<usize, BlockDbError> {
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

        Ok(block_db
            .add_item_or_ignore_by_unique_multi_index(
                MultiIndex::new(BlockDbTable::TICKER_HEIGHT_INDEX)
                    .with_value(&ticker)
                    .map_err(|err| BlockDbError::table_err(&ticker, err.to_string()))?
                    .with_value(BeBigUint::from(height))
                    .map_err(|err| BlockDbError::table_err(&ticker, err.to_string()))?,
                &BlockDbTable {
                    height,
                    data: cb_bytes,
                    ticker: ticker.clone(),
                },
            )
            .await
            .map_err(|err| BlockDbError::add_err(&ticker, err.to_string(), height))?
            .item_id() as usize)
    }

    pub async fn rewind_to_height(&self, height: u32) -> Result<usize, BlockDbError> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| BlockDbError::remove_err(&ticker, err.to_string(), height))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| BlockDbError::remove_err(&ticker, err.to_string(), height))?;
        let block_db = db_transaction
            .table::<BlockDbTable>()
            .await
            .map_err(|err| BlockDbError::remove_err(&ticker, err.to_string(), height))?;

        let get_latest_block = self.get_latest_block().await?;
        let height_to_remove_from = height + 1;
        for i in height_to_remove_from..=get_latest_block {
            let index_keys = MultiIndex::new(BlockDbTable::TICKER_HEIGHT_INDEX)
                .with_value(&ticker)
                .map_err(|err| BlockDbError::table_err(&ticker, err.to_string()))?
                .with_value(BeBigUint::from(height))
                .map_err(|err| BlockDbError::table_err(&ticker, err.to_string()))?;

            block_db
                .delete_item_by_unique_multi_index(index_keys)
                .await
                .map_err(|err| BlockDbError::remove_err(&ticker, err.to_string(), i))?;
        }

        Ok((height_to_remove_from + get_latest_block) as usize)
    }

    pub async fn query_blocks_by_limit(&self, limit: Option<u32>) -> Result<Vec<CompactBlockRow>, BlockDbError> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| BlockDbError::init_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| BlockDbError::init_err(&ticker, err.to_string()))?;
        let block_db = db_transaction
            .table::<BlockDbTable>()
            .await
            .map_err(|err| BlockDbError::init_err(&ticker, err.to_string()))?;

        // Fetch CompactBlocks that are needed for scanning.
        let blocks = block_db
            .get_items("ticker", &ticker)
            .await
            .map_err(|err| BlockDbError::get_err(&ticker, err.to_string()))?;

        let mut blocks_to_scan = vec![];
        for (_, block) in blocks {
            if let Some(limit) = limit {
                if block.height > limit {
                    break;
                }
            };

            blocks_to_scan.push(CompactBlockRow {
                height: block.height.into(),
                data: block.data,
            });
        }

        Ok(blocks_to_scan)
    }

    pub(crate) async fn process_blocks_with_mode(
        &self,
        _params: ZcoinConsensusParams,
        _mode: BlockProcessingMode,
        _validate_from: Option<(BlockHeight, BlockHash)>,
        _limit: Option<u32>,
    ) -> Result<(), BlockDbError> {
        //        let mut from_height = match &mode {
        //            BlockProcessingMode::Validate => validate_from
        //                .map(|(height, _)| height)
        //                .unwrap_or(BlockHeight::from_u32(params.sapling_activation_height) - 1),
        //            BlockProcessingMode::Scan(data) => data.block_height_extrema().await.map(|opt| {
        //                opt.map(|(_, max)| max)
        //                    .unwrap_or(BlockHeight::from_u32(params.sapling_activation_height) - 1)
        //            })?,
        //        };
        //
        //        let blocks = self.query_blocks_by_limit(from_height, limit).await?;
        //
        //        let mut prev_height = from_height;
        //        let mut prev_hash: Option<BlockHash> = validate_from.map(|(_, hash)| hash);
        //
        //        for block in blocks {
        //            if let Some(limit) = limit {
        //                if block.height > limit {
        //                    break;
        //                }
        //            }
        //
        //            if block.height < u32::from(from_height) {
        //                continue;
        //            }
        //
        //            let cbr = block.clone();
        //            let block = CompactBlock::parse_from_bytes(&cbr.data).map_err(ChainError::from)?;
        //
        //            if block.height() != cbr.height {
        //                return Err(BlockDbError::CorruptedData(format!(
        //                    "Block height {} did not match row's height field value {}",
        //                    block.height(),
        //                    cbr.height
        //                )));
        //            }
        //
        //            match &mode.clone() {
        //                BlockProcessingMode::Validate => {
        //                    validate_chain(block, &mut prev_height, &mut prev_hash).await?;
        //                },
        //                BlockProcessingMode::Scan(data) => {
        //                    scan_cached_block(data.clone(), &params, &block, &mut from_height).await?;
        //                },
        //            }
        //        }
        Ok(())
    }
}
