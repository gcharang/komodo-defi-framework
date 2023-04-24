use crate::eth::get_eth_address;
use crate::nft::nft_errors::GetNftInfoError;
use crate::nft::nft_structs::{Chain, ConvertChain, Nft, NftTransferHistory, NftWrapper};
use crate::nft::{send_moralis_request, FORMAT_DECIMAL_MORALIS, URL_MORALIS};
use async_trait::async_trait;
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::{MmError, MmResult};
use mm2_err_handle::mm_error::{NotEqual, NotMmError};
use serde::{Deserialize, Serialize};
use std::format;

#[cfg(not(target_arch = "wasm32"))] pub mod sql_storage;
#[cfg(target_arch = "wasm32")] pub mod wasm_storage;

pub trait NftStorageError: std::fmt::Debug + NotMmError + NotEqual + Send {}

#[async_trait]
pub trait NftListStorageOps {
    type Error: NftStorageError;

    /// Initializes tables in storage for the specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_nft_list(&self, ctx: &MmArc, chain: &Chain) -> MmResult<Vec<Nft>, Self::Error>;

    async fn add_nfts_to_list<I>(&self, chain: &Chain, nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send;

    async fn remove_nft_from_list(&self, nft: Nft) -> MmResult<(), Self::Error>;
}

#[async_trait]
pub trait NftTxHistoryStorageOps {
    type Error: NftStorageError;

    /// Initializes tables in storage for the specified chain type.
    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Whether tables are initialized for the specified chain.
    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    async fn get_tx_history(&self, ctx: &MmArc, chain: &Chain) -> MmResult<Vec<NftTransferHistory>, Self::Error>;

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

#[allow(dead_code)]
async fn get_moralis_nft_list(ctx: &MmArc, chain: &Chain) -> MmResult<Vec<Nft>, GetNftInfoError> {
    let api_key = ctx.conf["api_key"]
        .as_str()
        .ok_or_else(|| MmError::new(GetNftInfoError::ApiKeyError))?;

    let mut res_list = Vec::new();

    let (coin_str, chain_str) = chain.to_ticker_chain();
    let my_address = get_eth_address(ctx, &coin_str).await?;
    let uri_without_cursor = format!(
        "{}{}/nft?chain={}&{}",
        URL_MORALIS, my_address.wallet_address, chain_str, FORMAT_DECIMAL_MORALIS
    );

    // The cursor returned in the previous response (used for getting the next page).
    let mut cursor = String::new();
    loop {
        let uri = format!("{}{}", uri_without_cursor, cursor);
        let response = send_moralis_request(uri.as_str(), api_key).await?;
        if let Some(nfts_list) = response["result"].as_array() {
            for nft_json in nfts_list {
                let nft_wrapper: NftWrapper = serde_json::from_str(&nft_json.to_string())?;
                let nft = Nft {
                    chain: *chain,
                    token_address: nft_wrapper.token_address,
                    token_id: nft_wrapper.token_id.0,
                    amount: nft_wrapper.amount.0,
                    owner_of: nft_wrapper.owner_of,
                    token_hash: nft_wrapper.token_hash,
                    block_number_minted: *nft_wrapper.block_number_minted,
                    block_number: *nft_wrapper.block_number,
                    contract_type: nft_wrapper.contract_type.map(|v| v.0),
                    name: nft_wrapper.name,
                    symbol: nft_wrapper.symbol,
                    token_uri: nft_wrapper.token_uri,
                    metadata: nft_wrapper.metadata,
                    last_token_uri_sync: nft_wrapper.last_token_uri_sync,
                    last_metadata_sync: nft_wrapper.last_metadata_sync,
                    minter_address: nft_wrapper.minter_address,
                    possible_spam: nft_wrapper.possible_spam,
                };
                // collect NFTs from the page
                res_list.push(nft);
            }
            // if cursor is not null, there are other NFTs on next page,
            // and we need to send new request with cursor to get info from the next page.
            if let Some(cursor_res) = response["cursor"].as_str() {
                cursor = format!("{}{}", "&cursor=", cursor_res);
                continue;
            } else {
                break;
            }
        }
    }

    drop_mutability!(res_list);
    Ok(res_list)
}
