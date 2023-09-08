use crate::nft::nft_structs::{Chain, Nft, NftList, NftListFilters, NftTokenAddrId, NftTransferHistory,
                              NftTransferHistoryFilters, NftsTransferHistoryList, TransferMeta};
use crate::WithdrawError;
use async_trait::async_trait;
use derive_more::Display;
use ethereum_types::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::MmResult;
use mm2_err_handle::mm_error::{NotEqual, NotMmError};
use mm2_number::BigDecimal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::num::NonZeroUsize;

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod db_test_helpers;
#[cfg(not(target_arch = "wasm32"))] pub(crate) mod sql_storage;
#[cfg(target_arch = "wasm32")] pub(crate) mod wasm;

/// Represents the outcome of an attempt to remove an NFT.
#[derive(Debug, PartialEq)]
pub enum RemoveNftResult {
    /// Indicates that the NFT was successfully removed.
    NftRemoved,
    /// Indicates that the NFT did not exist in the storage.
    NftDidNotExist,
}

/// Defines the standard errors that can occur in NFT storage operations
pub trait NftStorageError: std::fmt::Debug + NotMmError + NotEqual + Send {}

/// Conversion trait from `NftStorageError` to `WithdrawError`.
impl<T: NftStorageError> From<T> for WithdrawError {
    fn from(err: T) -> Self { WithdrawError::DbError(format!("{:?}", err)) }
}

/// Provides asynchronous operations for handling and querying NFT listings.
#[async_trait]
pub trait NftListStorageOps {
    type Error: NftStorageError;

    /// Prepares the storage by initializing required tables for a specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_nft_list(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
        filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error>;

    async fn add_nfts_to_list<I>(&self, chain: Chain, nfts: I, last_scanned_block: u64) -> MmResult<(), Self::Error>
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
        scanned_block: u64,
    ) -> MmResult<RemoveNftResult, Self::Error>;

    async fn get_nft_amount(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<String>, Self::Error>;

    async fn refresh_nft_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error>;

    /// `get_last_block_number` function returns the height of last block in NFT LIST table
    async fn get_last_block_number(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error>;

    /// `get_last_scanned_block` function returns the height of last scanned block
    /// when token was added or removed from MFT LIST table.
    async fn get_last_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error>;

    /// `update_nft_amount` function sets a new amount of a particular token in NFT LIST table
    async fn update_nft_amount(&self, chain: &Chain, nft: Nft, scanned_block: u64) -> MmResult<(), Self::Error>;

    async fn update_nft_amount_and_block_number(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error>;

    /// `get_nfts_by_token_address` function returns list of NFTs which have specified token address.
    async fn get_nfts_by_token_address(&self, chain: Chain, token_address: String) -> MmResult<Vec<Nft>, Self::Error>;

    /// `update_nft_spam_by_token_address` function updates `possible_spam` field in NFTs which have specified token address.
    async fn update_nft_spam_by_token_address(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error>;
}

/// Provides asynchronous operations related to the history of NFT transfers.
#[async_trait]
pub trait NftTransferHistoryStorageOps {
    type Error: NftStorageError;

    /// Prepares the storage by initializing required tables for a specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_transfer_history(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
        filters: Option<NftTransferHistoryFilters>,
    ) -> MmResult<NftsTransferHistoryList, Self::Error>;

    async fn add_transfers_to_history<I>(&self, chain: Chain, transfers: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send;

    async fn get_last_block_number(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error>;

    /// `get_transfers_from_block` function returns transfers sorted by
    /// block_number in ascending order. It is needed to update the NFT LIST table correctly.
    async fn get_transfers_from_block(
        &self,
        chain: Chain,
        from_block: u64,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error>;

    async fn get_transfers_by_token_addr_id(
        &self,
        chain: Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error>;

    async fn get_transfer_by_tx_hash_and_log_index(
        &self,
        chain: &Chain,
        transaction_hash: String,
        log_index: u32,
    ) -> MmResult<Option<NftTransferHistory>, Self::Error>;

    async fn update_transfers_meta_by_token_addr_id(
        &self,
        chain: &Chain,
        transfer_meta: TransferMeta,
    ) -> MmResult<(), Self::Error>;

    async fn get_transfers_with_empty_meta(&self, chain: Chain) -> MmResult<Vec<NftTokenAddrId>, Self::Error>;

    /// `get_transfers_by_token_address` function returns list of NFT transfers which have specified token address.
    async fn get_transfers_by_token_address(
        &self,
        chain: Chain,
        token_address: String,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error>;

    /// `update_transfer_spam_by_token_address` function updates `possible_spam` field in NFT transfers which have specified token address.
    async fn update_transfer_spam_by_token_address(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error>;

    /// `get_token_addresses` return all unique token addresses.
    async fn get_token_addresses(&self, chain: Chain) -> MmResult<HashSet<Address>, Self::Error>;
}

/// Represents potential errors that can occur when creating an NFT storage.
#[derive(Debug, Deserialize, Display, Serialize)]
pub enum CreateNftStorageError {
    Internal(String),
}

impl From<CreateNftStorageError> for WithdrawError {
    fn from(e: CreateNftStorageError) -> Self {
        match e {
            CreateNftStorageError::Internal(err) => WithdrawError::InternalError(err),
        }
    }
}

/// `NftStorageBuilder` is used to create an instance that implements the [`NftListStorageOps`]
/// and [`NftTransferHistoryStorageOps`] traits.
pub struct NftStorageBuilder<'a> {
    ctx: &'a MmArc,
}

impl<'a> NftStorageBuilder<'a> {
    /// Creates a new `NftStorageBuilder` instance with the provided context.
    #[inline]
    pub fn new(ctx: &MmArc) -> NftStorageBuilder<'_> { NftStorageBuilder { ctx } }

    /// `build` function is used to build nft storage which implements [`NftListStorageOps`] and [`NftTransferHistoryStorageOps`] traits.
    #[inline]
    pub fn build(&self) -> MmResult<impl NftListStorageOps + NftTransferHistoryStorageOps, CreateNftStorageError> {
        #[cfg(target_arch = "wasm32")]
        return wasm::wasm_storage::IndexedDbNftStorage::new(self.ctx);
        #[cfg(not(target_arch = "wasm32"))]
        sql_storage::SqliteNftStorage::new(self.ctx)
    }
}

/// `get_offset_limit` function calculates offset and limit for final result if we use pagination.
fn get_offset_limit(max: bool, limit: usize, page_number: Option<NonZeroUsize>, total_count: usize) -> (usize, usize) {
    if max {
        return (0, total_count);
    }
    match page_number {
        Some(page) => ((page.get() - 1) * limit, limit),
        None => (0, limit),
    }
}

/// `NftDetailsJson` structure contains immutable parameters that are not needed for queries.
/// This is what `details_json` string contains in db table.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct NftDetailsJson {
    pub(crate) owner_of: Address,
    pub(crate) token_hash: Option<String>,
    pub(crate) minter_address: Option<String>,
    pub(crate) block_number_minted: Option<u64>,
}

/// `TransferDetailsJson` structure contains immutable parameters that are not needed for queries.
/// This is what `details_json` string contains in db table.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct TransferDetailsJson {
    pub(crate) block_hash: Option<String>,
    pub(crate) transaction_index: Option<u32>,
    pub(crate) value: Option<BigDecimal>,
    pub(crate) transaction_type: Option<String>,
    pub(crate) verified: Option<u32>,
    pub(crate) operator: Option<String>,
    pub(crate) from_address: Address,
    pub(crate) to_address: Address,
}
