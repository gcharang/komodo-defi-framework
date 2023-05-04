#[cfg(target_arch = "wasm32")] mod indexeddb;

use mm2_core::mm_ctx::MmArc;
use protobuf::Message;
use std::path::Path;
use std::sync::{Arc, Mutex};
use zcash_primitives::consensus::BlockHeight;
cfg_native!(
    use db_common::sqlite::rusqlite::{params, Connection, NO_PARAMS};
    use db_common::sqlite::{query_single_row, run_optimization_pragmas};
    use zcash_client_backend::data_api::BlockSource;
    use zcash_client_backend::proto::compact_formats::CompactBlock;
    use zcash_client_sqlite::error::SqliteClientError as ZcashClientError;
);

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct CompactBlockRow {
    height: BlockHeight,
    data: Vec<u8>,
}

#[allow(unused)]
#[derive(Debug, Display)]
pub enum BlockDbError {
    SqliteError(String),
    IndexedDBError(String),
}

/// A wrapper for the SQLite connection to the block cache database.
#[allow(unused)]
pub struct BlockDbImpl {
    #[cfg(not(target_arch = "wasm32"))]
    pub db: Arc<Mutex<Connection>>,
    #[cfg(target_arch = "wasm32")]
    pub db: SharedDb<BlockDbInner>,
    ticker: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl BlockDbImpl {
    pub fn new_from_path(_ctx: MmArc, ticker: String, path: impl AsRef<Path>) -> Result<Self, BlockDbError> {
        let conn = Connection::open(path).map_err(|err| BlockDbError::SqliteError(err.to_string()))?;
        run_optimization_pragmas(&conn).map_err(|err| BlockDbError::SqliteError(err.to_string()))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS compactblocks (
            height INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        )",
            NO_PARAMS,
        )
        .map_err(|err| BlockDbError::SqliteError(err.to_string()))?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            ticker,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl BlockDbImpl {
    pub(crate) fn get_latest_block(&self) -> Result<u32, ZcashClientError> {
        Ok(query_single_row(
            &self.db.lock().unwrap(),
            "SELECT height FROM compactblocks ORDER BY height DESC LIMIT 1",
            NO_PARAMS,
            |row| row.get(0),
        )?
        .unwrap_or(0))
    }

    pub(crate) fn insert_block(&self, height: u32, cb_bytes: Vec<u8>) -> Result<usize, ZcashClientError> {
        self.db
            .lock()
            .unwrap()
            .prepare("INSERT INTO compactblocks (height, data) VALUES (?, ?)")
            .map_err(ZcashClientError::DbError)?
            .execute(params![height, cb_bytes])
            .map_err(ZcashClientError::DbError)
    }

    pub(crate) fn rewind_to_height(&self, height: u32) -> Result<usize, ZcashClientError> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE from compactblocks WHERE height > ?1", [height])
            .map_err(ZcashClientError::DbError)
    }

    fn with_blocks<F>(
        &self,
        from_height: BlockHeight,
        limit: Option<u32>,
        mut with_row: F,
    ) -> Result<(), ZcashClientError>
    where
        F: FnMut(CompactBlock) -> Result<(), ZcashClientError>,
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
            let block = CompactBlock::parse_from_bytes(&cbr.data)
                .map_err(zcash_client_backend::data_api::error::Error::from)?;

            if block.height() != cbr.height {
                return Err(ZcashClientError::CorruptedData(format!(
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

cfg_wasm32!(
    use super::*;
    use crate::z_coin::storage::blockdb::indexeddb::BlockDbInner;
    use mm2_db::indexed_db::{ConstructibleDb, DbLocked, IndexedDb, SharedDb};
);

#[cfg(target_arch = "wasm32")]
pub type BlockDbRes<T> = MmResult<T, BlockDbError>;
#[cfg(target_arch = "wasm32")]
pub type BlockDbInnerLocked<'a> = DbLocked<'a, BlockDbInner>;

#[cfg(target_arch = "wasm32")]
impl BlockDbImpl {
    #[cfg(target_arch = "wasm32")]
    pub fn new_from_path(ctx: MmArc, ticker: String, _path: impl AsRef<Path>) -> Result<Self, BlockDbError> {
        Ok(Self {
            db: ConstructibleDb::new(&ctx).into_shared(),
            ticker,
        })
    }

    #[cfg(target_arch = "wasm32")]
    async fn lock_db(&self) -> BlockDbRes<BlockDbInnerLocked<'_>> {
        self.db
            .get_or_initialize()
            .await
            .mm_err(|err| BlockDbError::IndexedDBError(err.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
impl BlockDbImpl {
    pub fn get_latest_block(&self) -> Result<u32, ZcashClientError> { todo!() }

    pub fn insert_block(&self, height: u32, cb_bytes: Vec<u8>) -> Result<usize, ZcashClientError> { todo!() }

    pub fn rewind_to_height(&self, height: u32) -> Result<usize, ZcashClientError> { todo!() }

    pub fn with_blocks<F>(
        &self,
        from_height: BlockHeight,
        limit: Option<u32>,
        mut with_row: F,
    ) -> Result<(), ZcashClientError>
    where
        F: FnMut(CompactBlock) -> Result<(), ZcashClientError>,
    {
        todo!()
    }
}

impl BlockSource for BlockDbImpl {
    type Error = ZcashClientError;

    fn with_blocks<F>(&self, from_height: BlockHeight, limit: Option<u32>, with_row: F) -> Result<(), Self::Error>
    where
        F: FnMut(CompactBlock) -> Result<(), Self::Error>,
    {
        self.with_blocks(from_height, limit, with_row)
    }
}
