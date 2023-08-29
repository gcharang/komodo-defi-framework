use crate::z_coin::ZcoinClientInitError;

cfg_native!(
    use crate::z_coin::ZcoinConsensusParams;

    pub mod wallet_sql_storage;
    use zcash_client_sqlite::with_async::WalletDbAsync;
);

cfg_wasm32!(
    pub mod wallet_idb_storage;
    use wallet_idb_storage::WalletIndexedDb;
);

#[derive(Debug, Display)]
pub enum WalletDbError {
    ZcoinClientInitError(ZcoinClientInitError),
    ZCoinBuildError(String),
    IndexedDBError(String),
}

#[derive(Clone)]
pub struct WalletDbShared {
    #[cfg(not(target_arch = "wasm32"))]
    pub db: WalletDbAsync<ZcoinConsensusParams>,
    #[cfg(target_arch = "wasm32")]
    pub db: WalletIndexedDb,
    #[allow(unused)]
    ticker: String,
}
