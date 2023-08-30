use crate::z_coin::storage::{scan_cached_block, validate_chain, BlockDbImpl, BlockProcessingMode, CompactBlockRow,
                             ValidateBlocksError, ZcoinStorageError};
use crate::z_coin::ZcoinConsensusParams;

use common::async_blocking;
use db_common::sqlite::rusqlite::{params, Connection};
use db_common::sqlite::{query_single_row, run_optimization_pragmas, rusqlite};
use itertools::Itertools;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use protobuf::Message;
use std::path::Path;
use std::sync::{Arc, Mutex};
use zcash_client_backend::data_api::error::Error as ChainError;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_sqlite::error::{SqliteClientError as ZcashClientError, SqliteClientError};
use zcash_client_sqlite::NoteId;
use zcash_extras::WalletRead;
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;

impl From<ZcashClientError> for ZcoinStorageError {
    fn from(value: ZcashClientError) -> Self {
        match value {
            SqliteClientError::CorruptedData(err) => Self::CorruptedData(err),
            SqliteClientError::IncorrectHrpExtFvk => Self::IncorrectHrpExtFvk,
            SqliteClientError::InvalidNote => Self::InvalidNote(value.to_string()),
            SqliteClientError::InvalidNoteId => Self::InvalidNoteId(value.to_string()),
            SqliteClientError::TableNotEmpty => Self::TableNotEmpty(value.to_string()),
            SqliteClientError::Bech32(err) => Self::DecodingError(err.to_string()),
            SqliteClientError::Base58(err) => Self::DecodingError(err.to_string()),
            SqliteClientError::DbError(err) => Self::DecodingError(err.to_string()),
            SqliteClientError::Io(err) => Self::IoError(err.to_string()),
            SqliteClientError::InvalidMemo(err) => Self::InvalidMemo(err.to_string()),
            SqliteClientError::BackendError(err) => Self::BackendError(err.to_string()),
        }
    }
}

impl From<ValidateBlocksError> for ZcoinStorageError {
    fn from(value: ValidateBlocksError) -> Self { Self::ValidateBlocksError(value) }
}

impl From<ChainError<NoteId>> for ZcoinStorageError {
    fn from(value: ChainError<NoteId>) -> Self { Self::SqliteError(ZcashClientError::from(value)) }
}

impl BlockDbImpl {
    #[cfg(all(not(test)))]
    pub async fn new(_ctx: MmArc, ticker: String, path: Option<impl AsRef<Path>>) -> MmResult<Self, ZcoinStorageError> {
        let conn = Connection::open(path.unwrap()).map_to_mm(|err| ZcoinStorageError::DbError(err.to_string()))?;
        run_optimization_pragmas(&conn).map_to_mm(|err| ZcoinStorageError::DbError(err.to_string()))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS compactblocks (
            height INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        )",
            [],
        )
        .map_to_mm(|err| ZcoinStorageError::DbError(err.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            ticker,
        })
    }

    #[cfg(all(test))]
    pub(crate) async fn new(
        ctx: MmArc,
        ticker: String,
        _path: Option<impl AsRef<Path>>,
    ) -> MmResult<Self, ZcoinStorageError> {
        let conn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let conn = ctx.sqlite_connection.clone_or(conn);
        let clone_db = conn.clone();

        async_blocking(move || {
            let clone_db = clone_db.lock().unwrap();
            run_optimization_pragmas(&clone_db).map_err(|err| ZcoinStorageError::DbError(err.to_string()))?;
            clone_db
                .execute(
                    "CREATE TABLE IF NOT EXISTS compactblocks (
            height INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        )",
                    [],
                )
                .map_to_mm(|err| ZcoinStorageError::DbError(err.to_string()))?;

            Ok(BlockDbImpl { db: conn, ticker })
        })
        .await
    }

    pub(crate) async fn get_latest_block(&self) -> MmResult<u32, ZcashClientError> {
        Ok(query_single_row(
            &self.db.lock().unwrap(),
            "SELECT height FROM compactblocks ORDER BY height DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
        .unwrap_or(0))
    }

    pub(crate) async fn insert_block(&self, height: u32, cb_bytes: Vec<u8>) -> MmResult<usize, ZcoinStorageError> {
        let db = self.db.clone();
        async_blocking(move || {
            let db = db.lock().unwrap();
            let insert = db
                .prepare("INSERT INTO compactblocks (height, data) VALUES (?, ?)")
                .map_to_mm(|err| ZcoinStorageError::AddToStorageErr(err.to_string()))?
                .execute(params![height, cb_bytes])
                .map_to_mm(|err| ZcoinStorageError::AddToStorageErr(err.to_string()))?;

            Ok(insert)
        })
        .await
    }

    pub(crate) async fn rewind_to_height(&self, height: u32) -> MmResult<usize, ZcoinStorageError> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE from compactblocks WHERE height > ?1", [height])
            .map_to_mm(|err| ZcoinStorageError::RemoveFromStorageErr(err.to_string()))
    }

    pub(crate) async fn query_blocks_by_limit(
        &self,
        from_height: BlockHeight,
        limit: Option<u32>,
    ) -> MmResult<Vec<rusqlite::Result<CompactBlockRow>>, ZcoinStorageError> {
        let db = self.db.clone();
        async_blocking(move || {
            // Fetch the CompactBlocks we need to scan
            let db = db.lock().unwrap();
            let mut stmt_blocks = db
                .prepare(
                    "SELECT height, data FROM compactblocks WHERE height > ? ORDER BY height ASC \
        LIMIT ?",
                )
                .map_to_mm(|err| ZcoinStorageError::AddToStorageErr(err.to_string()))?;

            let rows = stmt_blocks
                .query_map(
                    params![u32::from(from_height), limit.unwrap_or(u32::max_value()),],
                    |row| {
                        Ok(CompactBlockRow {
                            height: BlockHeight::from_u32(row.get(0)?),
                            data: row.get(1)?,
                        })
                    },
                )
                .map_to_mm(|err| ZcoinStorageError::AddToStorageErr(err.to_string()))?;

            Ok(rows.collect_vec())
        })
        .await
    }

    pub(crate) async fn process_blocks_with_mode(
        &self,
        params: ZcoinConsensusParams,
        mode: BlockProcessingMode,
        validate_from: Option<(BlockHeight, BlockHash)>,
        limit: Option<u32>,
    ) -> MmResult<(), ZcoinStorageError> {
        let ticker = self.ticker.to_owned();
        let mut from_height = match &mode {
            BlockProcessingMode::Validate => validate_from
                .map(|(height, _)| height)
                .unwrap_or(BlockHeight::from_u32(params.sapling_activation_height) - 1),
            BlockProcessingMode::Scan(data) => {
                let data = data.inner();
                data.block_height_extrema().await.map(|opt| {
                    opt.map(|(_, max)| max)
                        .unwrap_or(BlockHeight::from_u32(params.sapling_activation_height) - 1)
                })?
            },
        };

        let rows = self.query_blocks_by_limit(from_height, limit).await?;

        let mut prev_height = from_height;
        let mut prev_hash: Option<BlockHash> = validate_from.map(|(_, hash)| hash);

        for row_result in rows {
            let cbr = row_result.map_err(|err| ZcoinStorageError::AddToStorageErr(err.to_string()))?;
            let block = CompactBlock::parse_from_bytes(&cbr.data)
                .map_err(|err| ZcoinStorageError::ChainError(err.to_string()))?;

            if block.height() != cbr.height {
                return MmError::err(ZcoinStorageError::CorruptedData(format!(
                    "{ticker}, Block height {} did not match row's height field value {}",
                    block.height(),
                    cbr.height
                )));
            }

            match &mode.clone() {
                BlockProcessingMode::Validate => {
                    validate_chain(block, &mut prev_height, &mut prev_hash).await?;
                },
                BlockProcessingMode::Scan(data) => {
                    scan_cached_block(data.clone(), &params, &block, &mut from_height).await?;
                },
            }
        }
        Ok(())
    }
}
