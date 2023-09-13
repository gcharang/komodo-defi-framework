use crate::nft::storage::wasm::wasm_storage::{LastScannedBlockTable, NftListTable, NftTransferHistoryTable};
use async_trait::async_trait;
use mm2_db::indexed_db::InitDbResult;
use mm2_db::indexed_db::{DbIdentifier, DbInstance, DbLocked, IndexedDb, IndexedDbBuilder};

const DB_NAME: &str = "nft_cache";
const DB_VERSION: u32 = 1;

/// Represents a locked instance of the `NftCacheIDB` database.
///
/// This type ensures that while the database is being accessed or modified,
/// no other operations can interfere, maintaining data integrity.
pub type NftCacheIDBLocked<'a> = DbLocked<'a, NftCacheIDB>;

pub struct LockedIndexedDbNftStorage<'a>(pub NftCacheIDBLocked<'a>);

/// Represents the IndexedDB instance specifically designed for caching NFT data.
///
/// This struct provides an abstraction over the raw IndexedDB, offering methods
/// to interact with the database and ensuring that the database is initialized with the
/// required tables and configurations.
pub struct NftCacheIDB {
    /// The underlying raw IndexedDb instance.
    inner: IndexedDb,
}

#[async_trait]
impl DbInstance for NftCacheIDB {
    /// Return the static name of the database.
    fn db_name() -> &'static str { DB_NAME }

    /// Initialize the `NftCacheIDB` database with the provided identifier.
    ///
    /// This method ensures that the database is properly set up with the correct version
    /// and has the required tables (`NftListTable`, `NftTransferHistoryTable`, and `LastScannedBlockTable`).
    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        let inner = IndexedDbBuilder::new(db_id)
            .with_version(DB_VERSION)
            .with_table::<NftListTable>()
            .with_table::<NftTransferHistoryTable>()
            .with_table::<LastScannedBlockTable>()
            .build()
            .await?;
        Ok(NftCacheIDB { inner })
    }
}

impl NftCacheIDB {
    /// Get a reference to the underlying `IndexedDb` instance.
    ///
    /// This method allows for direct interaction with the raw database, bypassing any abstractions.
    pub(crate) fn get_inner(&self) -> &IndexedDb { &self.inner }
}
