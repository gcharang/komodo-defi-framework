#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod blockdb_sql_storage;

#[cfg(not(target_arch = "wasm32"))]
use db_common::sqlite::rusqlite::Connection;
#[cfg(not(target_arch = "wasm32"))] use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use zcash_client_sqlite::error::SqliteClientError;

#[cfg(target_arch = "wasm32")]
pub(crate) mod blockdb_idb_storage;
#[cfg(target_arch = "wasm32")]
use blockdb_idb_storage::BlockDbInner;
#[cfg(target_arch = "wasm32")] use mm2_db::indexed_db::SharedDb;

/// A wrapper for the db connection to the block cache database in native and browser.
pub struct BlockDbImpl {
    #[cfg(not(target_arch = "wasm32"))]
    pub db: Arc<Mutex<Connection>>,
    #[cfg(target_arch = "wasm32")]
    pub db: SharedDb<BlockDbInner>,
    #[allow(unused)]
    ticker: String,
}

#[allow(unused)]
#[derive(Debug, Display)]
pub enum BlockDbError {
    #[cfg(not(target_arch = "wasm32"))]
    SqliteError(SqliteClientError),
    CorruptedData(String),
    #[cfg(target_arch = "wasm32")]
    #[display(fmt = "Error inserting {ticker:?} block data to db: {err} - height {height}")]
    AddToStorageErr {
        ticker: String,
        err: String,
        height: u32,
    },
    #[cfg(target_arch = "wasm32")]
    #[display(fmt = "Error getting {ticker} block height from storage: {err}")]
    BlockHeightNotFound {
        ticker: String,
        err: String,
    },
    #[display(fmt = "Error getting {ticker} block from storage: {err}")]
    GetFromStorageError {
        ticker: String,
        err: String,
    },
    #[cfg(target_arch = "wasm32")]
    #[display(fmt = "Storage Initialization err: {err} - ticker: {ticker}")]
    InitDbError {
        ticker: String,
        err: String,
    },
    #[cfg(target_arch = "wasm32")]
    #[display(fmt = "IndexedDB table err: {err} - ticker: {ticker}")]
    IdbTableError {
        ticker: String,
        err: String,
    },
}

impl BlockDbError {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn add_err(ticker: &str, err: String, height: u32) -> Self {
        Self::AddToStorageErr {
            ticker: ticker.to_string(),
            err,
            height,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn get_err(ticker: &str, err: String) -> Self {
        Self::GetFromStorageError {
            ticker: ticker.to_string(),
            err,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn init_err(ticker: &str, err: String) -> Self {
        Self::InitDbError {
            ticker: ticker.to_string(),
            err,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn not_found(ticker: &str, err: String) -> Self {
        Self::BlockHeightNotFound {
            ticker: ticker.to_string(),
            err,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn table_err(ticker: &str, err: String) -> Self {
        Self::IdbTableError {
            ticker: ticker.to_string(),
            err,
        }
    }
}
