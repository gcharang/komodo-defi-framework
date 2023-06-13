use crate::z_coin::storage::{BlockDbError, BlockDbImpl};

use common::block_on;
use db_common::sqlite::rusqlite::{params, Connection};
use db_common::sqlite::{query_single_row, run_optimization_pragmas};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use protobuf::Message;
use std::path::Path;
use std::sync::{Arc, Mutex};
use zcash_client_backend::data_api::error::Error as ChainError;
use zcash_client_backend::data_api::BlockSource;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_sqlite::error::{SqliteClientError as ZcashClientError, SqliteClientError};
use zcash_client_sqlite::NoteId;
use zcash_primitives::consensus::BlockHeight;

struct CompactBlockRow {
    height: BlockHeight,
    data: Vec<u8>,
}

impl From<SqliteClientError> for BlockDbError {
    fn from(value: SqliteClientError) -> Self { Self::SqliteError(value) }
}

impl From<ChainError<NoteId>> for BlockDbError {
    fn from(value: ChainError<NoteId>) -> Self { Self::SqliteError(SqliteClientError::from(value)) }
}

impl BlockDbImpl {
    #[cfg(all(not(test)))]
    pub async fn new(_ctx: MmArc, ticker: String, path: Option<impl AsRef<Path>>) -> MmResult<Self, BlockDbError> {
        let conn =
            Connection::open(path.unwrap()).map_err(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))?;
        run_optimization_pragmas(&conn).map_err(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS compactblocks (
            height INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        )",
            [],
        )
        .map_to_mm(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            ticker,
        })
    }

    #[cfg(all(test))]
    pub(crate) async fn new(ctx: MmArc, ticker: String, _path: Option<impl AsRef<Path>>) -> Result<Self, BlockDbError> {
        let conn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let conn = ctx.sqlite_connection.clone_or(conn);
        let clone_db = conn.clone();
        let clone_db = clone_db.lock().unwrap();
        run_optimization_pragmas(&clone_db).map_err(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))?;
        clone_db
            .execute(
                "CREATE TABLE IF NOT EXISTS compactblocks (
            height INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        )",
                [],
            )
            .map_to_mm(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))
            .unwrap();

        Ok(BlockDbImpl { db: conn, ticker })
    }

    pub(crate) async fn get_latest_block(&self) -> Result<u32, ZcashClientError> {
        Ok(query_single_row(
            &self.db.lock().unwrap(),
            "SELECT height FROM compactblocks ORDER BY height DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
        .unwrap_or(0))
    }

    pub(crate) async fn insert_block(&self, height: u32, cb_bytes: Vec<u8>) -> Result<usize, BlockDbError> {
        self.db
            .lock()
            .unwrap()
            .prepare("INSERT INTO compactblocks (height, data) VALUES (?, ?)")
            .map_err(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))?
            .execute(params![height, cb_bytes])
            .map_err(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))
    }

    pub(crate) async fn rewind_to_height(&self, height: u32) -> Result<usize, BlockDbError> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE from compactblocks WHERE height > ?1", [height])
            .map_err(|err| BlockDbError::SqliteError(SqliteClientError::from(err)))
    }

    async fn with_blocks<F>(
        &self,
        from_height: BlockHeight,
        limit: Option<u32>,
        mut with_row: F,
    ) -> Result<(), SqliteClientError>
    where
        F: FnMut(CompactBlock) -> Result<(), SqliteClientError>,
    {
        // Fetch the CompactBlocks we need to scan
        let stmt_blocks = self.db.lock().unwrap();
        let mut stmt_blocks = stmt_blocks.prepare(
            "SELECT height, data FROM compactblocks WHERE height > ? ORDER BY height ASC \
        LIMIT ?",
        )?;

        let rows = stmt_blocks.query_map(
            params![u32::from(from_height), limit.unwrap_or(u32::max_value()),],
            |row| {
                Ok(CompactBlockRow {
                    height: BlockHeight::from_u32(row.get(0)?),
                    data: row.get(1)?,
                })
            },
        )?;

        for row_result in rows {
            let cbr = row_result?;
            let block = CompactBlock::parse_from_bytes(&cbr.data).map_err(ChainError::from)?;

            if block.height() != cbr.height {
                return Err(SqliteClientError::CorruptedData(format!(
                    "Block height {} did not match row's height field value {}",
                    block.height(),
                    cbr.height
                )));
            }

            with_row(block)?;
        }

        Ok(())
    }
}

impl BlockSource for BlockDbImpl {
    type Error = SqliteClientError;

    fn with_blocks<F>(&self, from_height: BlockHeight, limit: Option<u32>, with_row: F) -> Result<(), Self::Error>
    where
        F: FnMut(CompactBlock) -> Result<(), Self::Error>,
    {
        block_on(self.with_blocks(from_height, limit, with_row))
    }
}
