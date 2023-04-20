use crate::nft::nft_structs::{Chain, Nft, NftList, NftTransferHistory, NftsTransferHistoryList};
use async_trait::async_trait;
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::NotMmError;
use mm2_err_handle::prelude::MmResult;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))] pub mod sql_storage;
#[cfg(target_arch = "wasm32")] pub mod wasm_storage;

pub trait NftListStorageError: std::fmt::Debug + NotMmError + Send {}
pub trait NftTxHistoryStorageError: std::fmt::Debug + NotMmError + Send {}

#[async_trait]
pub trait NftListStorageOps {
    type Error: NftListStorageError;

    /// Initializes tables in storage for the specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized_for(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_nft_list(&self, chain: &Chain) -> MmResult<NftList, Self::Error>;

    async fn add_nfts_to_list<I>(&self, chain: &Chain, nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send;

    async fn remove_nft_from_list(&self, nft: Nft) -> MmResult<(), Self::Error>;
}

#[async_trait]
pub trait NftTxHistoryStorageOps {
    type Error: NftTxHistoryStorageError;

    /// Initializes tables in storage for the specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized_for(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_tx_history(&self, chain: &Chain) -> MmResult<NftsTransferHistoryList, Self::Error>;

    async fn add_txs_to_history<I>(&self, chain: &Chain, nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send;
}

#[derive(Debug, Deserialize, Display, Serialize)]
pub enum CreateNftStorageError {
    Internal(String),
}

/// `NftStorageBuilder` is used to create an instance that implements the `NftListStorageOps`
/// and `NftTxHistoryStorageOps` traits.
pub struct NftStorageBuilder<'a> {
    ctx: &'a MmArc,
}

impl<'a> NftStorageBuilder<'a> {
    #[inline]
    pub fn new(ctx: &MmArc) -> NftStorageBuilder<'_> { NftStorageBuilder { ctx } }

    #[inline]
    pub fn build(self) -> MmResult<impl NftListStorageOps + NftTxHistoryStorageOps, CreateNftStorageError> {
        #[cfg(target_arch = "wasm32")]
        return wasm_storage::IndexedDbNftStorage::new(self.ctx);
        #[cfg(not(target_arch = "wasm32"))]
        sql_storage::SqliteNftStorage::new(self.ctx)
    }
}
