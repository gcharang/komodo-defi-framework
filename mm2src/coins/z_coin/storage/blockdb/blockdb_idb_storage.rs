use crate::z_coin::storage::validate_chain;
use crate::z_coin::storage::{BlockDbImpl, BlockProcessingMode, CompactBlockRow, ZcoinConsensusParams};
use crate::z_coin::z_coin_errors::ZcoinStorageError;

use async_trait::async_trait;
use mm2_core::mm_ctx::MmArc;
use mm2_db::indexed_db::{BeBigUint, DbIdentifier, DbInstance, DbUpgrader, IndexedDb, IndexedDbBuilder, InitDbResult,
                         MultiIndex, OnUpgradeResult, TableSignature};
use mm2_db::indexed_db::{ConstructibleDb, DbLocked};
use mm2_err_handle::prelude::*;
use num_traits::ToPrimitive;
use protobuf::Message;
use std::path::Path;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_extras::WalletRead;
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;

const DB_NAME: &str = "z_compactblocks_cache";
const DB_VERSION: u32 = 1;

pub type BlockDbRes<T> = MmResult<T, ZcoinStorageError>;
pub type BlockDbInnerLocked<'a> = DbLocked<'a, BlockDbInner>;

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

pub struct BlockDbInner(IndexedDb);

#[async_trait]
impl DbInstance for BlockDbInner {
    fn db_name() -> &'static str { DB_NAME }

    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        let inner = IndexedDbBuilder::new(db_id)
            .with_version(DB_VERSION)
            .with_table::<BlockDbTable>()
            .build()
            .await?;

        Ok(Self(inner))
    }
}

impl BlockDbInner {
    pub fn get_inner(&self) -> &IndexedDb { &self.0 }
}

impl BlockDbImpl {
    pub async fn new(ctx: MmArc, ticker: String, _path: Option<impl AsRef<Path>>) -> MmResult<Self, ZcoinStorageError> {
        Ok(Self {
            db: ConstructibleDb::new(&ctx).into_shared(),
            ticker,
        })
    }

    async fn lock_db(&self) -> BlockDbRes<BlockDbInnerLocked<'_>> {
        self.db
            .get_or_initialize()
            .await
            .mm_err(|err| ZcoinStorageError::DbError(err.to_string()))
    }

    /// Get latest block of the current active ZCOIN.
    pub async fn get_latest_block(&self) -> MmResult<u32, ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_db = db_transaction.table::<BlockDbTable>().await?;
        let maybe_height = block_db
            .cursor_builder()
            .only("ticker", &ticker)?
            .bound("height", 0u32, u32::MAX)
            .reverse()
            .open_cursor(BlockDbTable::TICKER_HEIGHT_INDEX)
            .await?
            .next()
            .await?;

        let maybe_height = maybe_height
            .map(|(_, item)| {
                item.height
                    .to_u32()
                    .ok_or_else(|| ZcoinStorageError::GetFromStorageError("height is too large".to_string()))
            })
            .transpose()?;

        let Some(height) = maybe_height else {
            return MmError::err(ZcoinStorageError::GetFromStorageError(format!("{ticker} block height not found")));
        };

        Ok(height)
    }

    /// Insert new block to BlockDbTable given the provided data.
    pub async fn insert_block(&self, height: u32, cb_bytes: Vec<u8>) -> MmResult<usize, ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_db = db_transaction.table::<BlockDbTable>().await?;

        let indexes = MultiIndex::new(BlockDbTable::TICKER_HEIGHT_INDEX)
            .with_value(&ticker)?
            .with_value(BeBigUint::from(height))?;
        let block = BlockDbTable {
            height,
            data: cb_bytes,
            ticker,
        };

        Ok(block_db.replace_item_by_unique_multi_index(indexes, &block).await? as usize)
    }

    /// Asynchronously rewinds the storage to a specified block height, effectively
    /// removing data beyond the specified height from the storage.    
    pub async fn rewind_to_height(&self, height: u32) -> MmResult<usize, ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_db = db_transaction.table::<BlockDbTable>().await?;

        let get_latest_block = self.get_latest_block().await?;
        let height_to_remove_from = height + 1;
        for i in height_to_remove_from..get_latest_block {
            let index_keys = MultiIndex::new(BlockDbTable::TICKER_HEIGHT_INDEX)
                .with_value(&self.ticker)?
                .with_value(BeBigUint::from(i))?;

            block_db.delete_item_by_unique_multi_index(index_keys).await?;
        }

        Ok((height_to_remove_from + get_latest_block) as usize)
    }

    /// Queries and retrieves a list of `CompactBlockRow` records from the database, starting
    /// from a specified block height and optionally limited by a maximum number of blocks.
    pub async fn query_blocks_by_limit(
        &self,
        from_height: BlockHeight,
        limit: Option<u32>,
    ) -> MmResult<Vec<CompactBlockRow>, ZcoinStorageError> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_db = db_transaction.table::<BlockDbTable>().await?;

        // Fetch CompactBlocks block_db are needed for scanning.
        let mut maybe_blocks = block_db
            .cursor_builder()
            .only("ticker", &self.ticker)?
            .bound("block", u32::from(from_height), limit.unwrap_or(u32::MAX))
            .open_cursor("ticker")
            .await?;

        let mut blocks_to_scan = vec![];
        while let Some((_, block)) = maybe_blocks.next().await? {
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

    /// Processes blockchain blocks with a specified mode of operation, such as validation or scanning.
    ///
    /// Processes blocks based on the provided `BlockProcessingMode` and other parameters,
    /// which may include a starting block height, validation criteria, and a processing limit.
    #[allow(unused)]
    pub(crate) async fn process_blocks_with_mode(
        &self,
        params: ZcoinConsensusParams,
        mode: BlockProcessingMode,
        validate_from: Option<(BlockHeight, BlockHash)>,
        limit: Option<u32>,
    ) -> MmResult<(), ZcoinStorageError> {
        // TODO: make from_height var mut after impl walletdb for wasm.
        let from_height = match &mode {
            BlockProcessingMode::Validate => validate_from
                .map(|(height, _)| height)
                .unwrap_or(BlockHeight::from_u32(params.sapling_activation_height) - 1),
            BlockProcessingMode::Scan(data) => data.inner().block_height_extrema().await.map(|opt| {
                opt.map(|(_, max)| max)
                    .unwrap_or(BlockHeight::from_u32(params.sapling_activation_height) - 1)
            })?,
        };

        let blocks = self.query_blocks_by_limit(from_height, limit).await?;

        let mut prev_height = from_height;
        let mut prev_hash: Option<BlockHash> = validate_from.map(|(_, hash)| hash);

        for block in blocks {
            if let Some(limit) = limit {
                if u32::from(block.height) > limit {
                    break;
                }
            }

            if block.height < from_height {
                continue;
            }

            let cbr = block;
            let block = CompactBlock::parse_from_bytes(&cbr.data)
                .map_to_mm(|err| ZcoinStorageError::DecodingError(err.to_string()))?;

            if block.height() != cbr.height {
                return MmError::err(ZcoinStorageError::CorruptedData(format!(
                    "Block height {} did not match row's height field value {}",
                    block.height(),
                    cbr.height
                )));
            }

            match &mode.clone() {
                BlockProcessingMode::Validate => {
                    validate_chain(block, &mut prev_height, &mut prev_hash).await?;
                },
                BlockProcessingMode::Scan(_data) => {
                    // TODO: uncomment after implementing walletdb for wasm.
                    // scan_cached_block(data.clone(), &params, &block, &mut from_height).await?;
                },
            }
        }
        Ok(())
    }
}
