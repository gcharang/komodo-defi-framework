use crate::nft::nft_structs::{Chain, Nft, NftList, NftTransferHistory, NftTxHistoryFilters, NftsTransferHistoryList};
use async_trait::async_trait;
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::MmResult;
use mm2_err_handle::mm_error::{NotEqual, NotMmError};
use mm2_number::BigDecimal;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[cfg(not(target_arch = "wasm32"))] pub mod sql_storage;
#[cfg(target_arch = "wasm32")] pub mod wasm_storage;

#[derive(Debug)]
pub enum RemoveNftResult {
    NftRemoved,
    NftDidNotExist,
}

pub trait NftStorageError: std::fmt::Debug + NotMmError + NotEqual + Send {}

#[async_trait]
pub trait NftListStorageOps {
    type Error: NftStorageError;

    /// Initializes tables in storage for the specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_nft_list(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
    ) -> MmResult<NftList, Self::Error>;

    async fn add_nfts_to_list<I>(&self, chain: &Chain, nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send;

    async fn get_nft(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<Nft>, Self::Error>;

    async fn remove_nft_from_list(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<RemoveNftResult, Self::Error>;

    async fn get_nft_amount(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<String>, Self::Error>;

    async fn refresh_nft_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error>;
}

#[async_trait]
pub trait NftTxHistoryStorageOps {
    type Error: NftStorageError;

    /// Initializes tables in storage for the specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_tx_history(
        &self,
        chain_addr: Vec<(Chain, String)>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
        filters: Option<NftTxHistoryFilters>,
    ) -> MmResult<NftsTransferHistoryList, Self::Error>;

    async fn add_txs_to_history<I>(&self, chain: &Chain, txs: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send;

    async fn get_latest_block_number(&self, chain: &Chain) -> MmResult<Option<u32>, Self::Error>;
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
