use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::{MmError, MmResult};
use url::Url;

pub(crate) mod nft_errors;
pub(crate) mod nft_structs;
#[cfg(any(test, target_arch = "wasm32"))] mod nft_tests;

use crate::{get_my_address, MyAddressReq, WithdrawError};
use nft_errors::{GetNftInfoError, UpdateNftError};
use nft_structs::{Chain, ContractType, ConvertChain, Nft, NftList, NftListReq, NftMetadataReq, NftTransferHistory,
                  NftTransferHistoryWrapper, NftTransfersReq, NftWrapper, NftsTransferHistoryList,
                  TransactionNftDetails, UpdateNftReq, WithdrawNftReq};

use crate::eth::{get_eth_address, withdraw_erc1155, withdraw_erc721};
use crate::nft::nft_structs::{TransferStatus, UriMeta};
use crate::nft_storage::{NftListStorageOps, NftStorageBuilder, NftTxHistoryStorageOps};
use common::APPLICATION_JSON;
use enum_from::EnumFromStringify;
use http::header::ACCEPT;
use mm2_err_handle::map_to_mm::MapToMmResult;
use mm2_net::transport::SlurpError;
use mm2_number::BigDecimal;
use serde_json::Value as Json;
use std::cmp::Ordering;

const MORALIS_API_ENDPOINT: &str = "api/v2";
/// query parameters for moralis request: The format of the token ID
const MORALIS_FORMAT_QUERY_NAME: &str = "format";
const MORALIS_FORMAT_QUERY_VALUE: &str = "decimal";
/// query parameters for moralis request: The transfer direction
const MORALIS_DIRECTION_QUERY_NAME: &str = "direction";
const MORALIS_DIRECTION_QUERY_VALUE: &str = "both";
// The minimum block number from which to get the transfers
const MORALIS_FROM_BLOCK_QUERY_NAME: &str = "from_block";

pub type WithdrawNftResult = Result<TransactionNftDetails, MmError<WithdrawError>>;

/// `get_nft_list` function returns list of NFTs on requested chains owned by user.
pub async fn get_nft_list(ctx: MmArc, req: NftListReq) -> MmResult<NftList, GetNftInfoError> {
    let storage = NftStorageBuilder::new(&ctx).build()?;
    for chain in req.chains.iter() {
        if !NftListStorageOps::is_initialized(&storage, chain).await? {
            NftListStorageOps::init(&storage, chain).await?;
        }
    }
    let nfts = storage
        .get_nft_list(req.chains, req.max, req.limit, req.page_number)
        .await?;
    Ok(nfts)
}

/// `get_nft_metadata` function returns info of one specific NFT.
pub async fn get_nft_metadata(ctx: MmArc, req: NftMetadataReq) -> MmResult<Nft, GetNftInfoError> {
    let storage = NftStorageBuilder::new(&ctx).build()?;
    if !NftListStorageOps::is_initialized(&storage, &req.chain).await? {
        NftListStorageOps::init(&storage, &req.chain).await?;
    }
    let nft = storage
        .get_nft(&req.chain, format!("{:#02x}", req.token_address), req.token_id.clone())
        .await?;
    if let Some(nft) = nft {
        Ok(nft)
    } else {
        MmError::err(GetNftInfoError::TokenNotFoundInWallet {
            token_address: format!("{:#02x}", req.token_address),
            token_id: req.token_id.to_string(),
        })
    }
}

/// `get_nft_transfers` function returns a transfer history of NFTs on requested chains owned by user.
pub async fn get_nft_transfers(ctx: MmArc, req: NftTransfersReq) -> MmResult<NftsTransferHistoryList, GetNftInfoError> {
    let storage = NftStorageBuilder::new(&ctx).build()?;
    for chain in req.chains.iter() {
        if !NftTxHistoryStorageOps::is_initialized(&storage, chain).await? {
            NftTxHistoryStorageOps::init(&storage, chain).await?;
        }
    }
    let transfer_history_list = storage
        .get_tx_history(req.chains, req.max, req.limit, req.page_number, req.filters)
        .await?;
    Ok(transfer_history_list)
}

/// `update_nft` function updates cache of nft transfer history and nft list.
pub async fn update_nft(ctx: MmArc, req: UpdateNftReq) -> MmResult<(), UpdateNftError> {
    let storage = NftStorageBuilder::new(&ctx).build()?;
    for chain in req.chains.iter() {
        let tx_history_initialized = NftTxHistoryStorageOps::is_initialized(&storage, chain).await?;
        let list_initialized = NftListStorageOps::is_initialized(&storage, chain).await?;

        if !tx_history_initialized {
            NftTxHistoryStorageOps::init(&storage, chain).await?;
            let nft_transfers = get_moralis_nft_transfers(&ctx, chain, None, &req.url).await?;
            storage.add_txs_to_history(chain, nft_transfers).await?;
        } else {
            let last_tx_block = NftTxHistoryStorageOps::get_last_block_number(&storage, chain).await?;
            let nft_transfers = get_moralis_nft_transfers(&ctx, chain, last_tx_block.map(|b| b + 1), &req.url).await?;
            storage.add_txs_to_history(chain, nft_transfers).await?;
        }

        if !list_initialized {
            NftListStorageOps::init(&storage, chain).await?;
            let nft_list = get_moralis_nft_list(&ctx, chain, &req.url).await?;
            let last_scanned_block = NftTxHistoryStorageOps::get_last_block_number(&storage, chain)
                .await?
                .unwrap_or(0);
            storage
                .add_nfts_to_list(chain, nft_list.clone(), last_scanned_block)
                .await?;
            // this will update only txs related to current nfts in wallet.
            update_meta_in_txs(&storage, chain, nft_list).await?;
            update_txs_with_empty_meta(&storage, chain, &req.url).await?;
        } else {
            let last_scanned_block = storage.get_last_scanned_block(chain).await?;
            let last_nft_block = NftListStorageOps::get_last_block_number(&storage, chain).await?;

            match (last_scanned_block, last_nft_block) {
                // if both block numbers exist, last scanned block should be equal
                // or higher than last block number from NFT LIST table.
                (Some(scanned_block), Some(nft_block)) => {
                    if scanned_block >= nft_block {
                        update_nft_list(ctx.clone(), &storage, chain, scanned_block + 1, &req.url).await?;
                        update_txs_with_empty_meta(&storage, chain, &req.url).await?;
                    } else {
                        return MmError::err(UpdateNftError::InvalidBlockOrder {
                            last_scanned_block: scanned_block.to_string(),
                            last_nft_block: nft_block.to_string(),
                        });
                    }
                },
                // If the last scanned block value is absent, we cannot accurately update the NFT cache.
                // This is because a situation may occur where a user doesn't transfer all ERC-1155 tokens,
                // resulting in the block number of NFT remaining unchanged.
                (None, Some(nft_block)) => {
                    return MmError::err(UpdateNftError::LastScannedBlockNotFound {
                        last_nft_block: nft_block.to_string(),
                    });
                },
                // if there are no rows in NFT LIST table or in both tables there are no rows
                // we can try to get all info from moralis.
                (Some(_), None) => {
                    let nfts = cache_nfts_from_moralis(&ctx, &storage, chain, &req.url).await?;
                    update_meta_in_txs(&storage, chain, nfts).await?;
                    update_txs_with_empty_meta(&storage, chain, &req.url).await?;
                },
                (None, None) => {
                    let nfts = cache_nfts_from_moralis(&ctx, &storage, chain, &req.url).await?;
                    update_meta_in_txs(&storage, chain, nfts).await?;
                    update_txs_with_empty_meta(&storage, chain, &req.url).await?;
                },
            }
        }
    }
    Ok(())
}

pub async fn refresh_nft_metadata(ctx: MmArc, req: NftMetadataReq) -> MmResult<(), UpdateNftError> {
    let moralis_meta = get_moralis_metadata(
        format!("{:#02x}", req.token_address),
        req.token_id.clone(),
        &req.chain,
        &req.url,
    )
    .await?;
    let storage = NftStorageBuilder::new(&ctx).build()?;
    let req = NftMetadataReq {
        token_address: req.token_address,
        token_id: req.token_id,
        chain: req.chain,
        url: req.url,
    };
    let mut nft_db = get_nft_metadata(ctx, req).await?;
    let uri_meta = try_get_uri_meta(&moralis_meta.token_uri).await?;
    nft_db.collection_name = moralis_meta.collection_name;
    nft_db.symbol = moralis_meta.symbol;
    nft_db.token_uri = moralis_meta.token_uri;
    nft_db.metadata = moralis_meta.metadata;
    nft_db.last_token_uri_sync = moralis_meta.last_token_uri_sync;
    nft_db.last_metadata_sync = moralis_meta.last_metadata_sync;
    nft_db.possible_spam = moralis_meta.possible_spam;
    nft_db.uri_meta = uri_meta;
    drop_mutability!(nft_db);
    storage
        .refresh_nft_metadata(&moralis_meta.chain, nft_db.clone())
        .await?;
    storage
        .update_txs_meta_by_token_addr_id(
            &nft_db.chain,
            nft_db.token_address,
            nft_db.token_id,
            nft_db.collection_name,
            nft_db.uri_meta.image,
            nft_db.uri_meta.token_name,
        )
        .await?;
    Ok(())
}

async fn get_moralis_nft_list(ctx: &MmArc, chain: &Chain, url: &Url) -> MmResult<Vec<Nft>, GetNftInfoError> {
    let mut res_list = Vec::new();
    let my_address = get_eth_address(ctx, &chain.to_ticker()).await?;

    let mut uri_without_cursor = url.clone();
    uri_without_cursor.set_path(MORALIS_API_ENDPOINT);
    uri_without_cursor
        .path_segments_mut()
        .map_to_mm(|_| GetNftInfoError::Internal("Invalid URI".to_string()))?
        .push(&my_address.wallet_address)
        .push("nft");
    uri_without_cursor
        .query_pairs_mut()
        .append_pair("chain", &chain.to_string())
        .append_pair(MORALIS_FORMAT_QUERY_NAME, MORALIS_FORMAT_QUERY_VALUE);
    drop_mutability!(uri_without_cursor);

    // The cursor returned in the previous response (used for getting the next page).
    let mut cursor = String::new();
    loop {
        let uri = format!("{}{}", uri_without_cursor, cursor);
        let response = send_request_to_uri(uri.as_str()).await?;
        if let Some(nfts_list) = response["result"].as_array() {
            for nft_json in nfts_list {
                let nft_wrapper: NftWrapper = serde_json::from_str(&nft_json.to_string())?;
                let uri_meta = try_get_uri_meta(&nft_wrapper.token_uri).await?;
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
                    collection_name: nft_wrapper.name,
                    symbol: nft_wrapper.symbol,
                    token_uri: nft_wrapper.token_uri,
                    metadata: nft_wrapper.metadata,
                    last_token_uri_sync: nft_wrapper.last_token_uri_sync,
                    last_metadata_sync: nft_wrapper.last_metadata_sync,
                    minter_address: nft_wrapper.minter_address,
                    possible_spam: nft_wrapper.possible_spam,
                    uri_meta,
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

async fn get_moralis_nft_transfers(
    ctx: &MmArc,
    chain: &Chain,
    from_block: Option<u32>,
    url: &Url,
) -> MmResult<Vec<NftTransferHistory>, GetNftInfoError> {
    let mut res_list = Vec::new();
    let my_address = get_eth_address(ctx, &chain.to_ticker()).await?;

    let mut uri_without_cursor = url.clone();
    uri_without_cursor.set_path(MORALIS_API_ENDPOINT);
    uri_without_cursor
        .path_segments_mut()
        .map_to_mm(|_| GetNftInfoError::Internal("Invalid URI".to_string()))?
        .push(&my_address.wallet_address)
        .push("nft")
        .push("transfers");
    let from_block = match from_block {
        Some(block) => block.to_string(),
        None => "0".into(),
    };
    uri_without_cursor
        .query_pairs_mut()
        .append_pair("chain", &chain.to_string())
        .append_pair(MORALIS_FORMAT_QUERY_NAME, MORALIS_FORMAT_QUERY_VALUE)
        .append_pair(MORALIS_DIRECTION_QUERY_NAME, MORALIS_DIRECTION_QUERY_VALUE)
        .append_pair(MORALIS_FROM_BLOCK_QUERY_NAME, &from_block);
    drop_mutability!(uri_without_cursor);

    // The cursor returned in the previous response (used for getting the next page).
    let mut cursor = String::new();
    loop {
        let uri = format!("{}{}", uri_without_cursor, cursor);
        let response = send_request_to_uri(uri.as_str()).await?;
        if let Some(transfer_list) = response["result"].as_array() {
            for transfer in transfer_list {
                let transfer_wrapper: NftTransferHistoryWrapper = serde_json::from_str(&transfer.to_string())?;
                let status = if my_address.wallet_address.to_lowercase() == transfer_wrapper.to_address {
                    TransferStatus::Receive
                } else {
                    TransferStatus::Send
                };
                let transfer_history = NftTransferHistory {
                    chain: *chain,
                    block_number: *transfer_wrapper.block_number,
                    block_timestamp: transfer_wrapper.block_timestamp,
                    block_hash: transfer_wrapper.block_hash,
                    transaction_hash: transfer_wrapper.transaction_hash,
                    transaction_index: transfer_wrapper.transaction_index,
                    log_index: transfer_wrapper.log_index,
                    value: transfer_wrapper.value.0,
                    contract_type: transfer_wrapper.contract_type.0,
                    transaction_type: transfer_wrapper.transaction_type,
                    token_address: transfer_wrapper.token_address,
                    token_id: transfer_wrapper.token_id.0,
                    collection_name: None,
                    image: None,
                    token_name: None,
                    from_address: transfer_wrapper.from_address,
                    to_address: transfer_wrapper.to_address,
                    status,
                    amount: transfer_wrapper.amount.0,
                    verified: transfer_wrapper.verified,
                    operator: transfer_wrapper.operator,
                    possible_spam: transfer_wrapper.possible_spam,
                };
                // collect NFTs transfers from the page
                res_list.push(transfer_history);
            }
            // if the cursor is not null, there are other NFTs transfers on next page,
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

/// **Caution:** ERC-1155 token can have a total supply more than 1, which means there could be several owners
/// of the same token. `get_nft_metadata` returns NFTs info with the most recent owner.
/// **Dont** use this function to get specific info about owner address, amount etc, you will get info not related to my_address.
async fn get_moralis_metadata(
    token_address: String,
    token_id: BigDecimal,
    chain: &Chain,
    url: &Url,
) -> MmResult<Nft, GetNftInfoError> {
    let mut uri = url.clone();
    uri.set_path(MORALIS_API_ENDPOINT);
    uri.path_segments_mut()
        .map_to_mm(|_| GetNftInfoError::Internal("Invalid URI".to_string()))?
        .push("nft")
        .push(&token_address)
        .push(&token_id.to_string());
    uri.query_pairs_mut()
        .append_pair("chain", &chain.to_string())
        .append_pair(MORALIS_FORMAT_QUERY_NAME, MORALIS_FORMAT_QUERY_VALUE);
    drop_mutability!(uri);

    let response = send_request_to_uri(uri.as_str()).await?;
    let nft_wrapper: NftWrapper = serde_json::from_str(&response.to_string())?;
    let uri_meta = try_get_uri_meta(&nft_wrapper.token_uri).await?;
    let nft_metadata = Nft {
        chain: *chain,
        token_address: nft_wrapper.token_address,
        token_id: nft_wrapper.token_id.0,
        amount: nft_wrapper.amount.0,
        owner_of: nft_wrapper.owner_of,
        token_hash: nft_wrapper.token_hash,
        block_number_minted: *nft_wrapper.block_number_minted,
        block_number: *nft_wrapper.block_number,
        contract_type: nft_wrapper.contract_type.map(|v| v.0),
        collection_name: nft_wrapper.name,
        symbol: nft_wrapper.symbol,
        token_uri: nft_wrapper.token_uri,
        metadata: nft_wrapper.metadata,
        last_token_uri_sync: nft_wrapper.last_token_uri_sync,
        last_metadata_sync: nft_wrapper.last_metadata_sync,
        minter_address: nft_wrapper.minter_address,
        possible_spam: nft_wrapper.possible_spam,
        uri_meta,
    };
    Ok(nft_metadata)
}

/// `withdraw_nft` function generates, signs and returns a transaction that transfers NFT
/// from my address to recipient's address.
/// This method generates a raw transaction which should then be broadcast using `send_raw_transaction`.
pub async fn withdraw_nft(ctx: MmArc, req: WithdrawNftReq) -> WithdrawNftResult {
    match req {
        WithdrawNftReq::WithdrawErc1155(erc1155_withdraw) => withdraw_erc1155(ctx, erc1155_withdraw).await,
        WithdrawNftReq::WithdrawErc721(erc721_withdraw) => withdraw_erc721(ctx, erc721_withdraw).await,
    }
}

#[derive(Clone, Debug, Deserialize, Display, EnumFromStringify, PartialEq, Serialize)]
enum GetInfoFromUriError {
    /// `http::Error` can appear on an HTTP request [`http::Builder::build`] building.
    #[from_stringify("http::Error")]
    #[display(fmt = "Invalid request: {}", _0)]
    InvalidRequest(String),
    #[display(fmt = "Transport: {}", _0)]
    Transport(String),
    #[from_stringify("serde_json::Error")]
    #[display(fmt = "Invalid response: {}", _0)]
    InvalidResponse(String),
    #[display(fmt = "Internal: {}", _0)]
    Internal(String),
}

impl From<SlurpError> for GetInfoFromUriError {
    fn from(e: SlurpError) -> Self {
        let error_str = e.to_string();
        match e {
            SlurpError::ErrorDeserializing { .. } => GetInfoFromUriError::InvalidResponse(error_str),
            SlurpError::Transport { .. } | SlurpError::Timeout { .. } => GetInfoFromUriError::Transport(error_str),
            SlurpError::Internal(_) | SlurpError::InvalidRequest(_) => GetInfoFromUriError::Internal(error_str),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn send_request_to_uri(uri: &str) -> MmResult<Json, GetInfoFromUriError> {
    use http::header::HeaderValue;
    use mm2_net::transport::slurp_req_body;

    let request = http::Request::builder()
        .method("GET")
        .uri(uri)
        .header(ACCEPT, HeaderValue::from_static(APPLICATION_JSON))
        .body(hyper::Body::from(""))?;

    let (status, _header, body) = slurp_req_body(request).await?;
    if !status.is_success() {
        return Err(MmError::new(GetInfoFromUriError::Transport(format!(
            "Response !200 from {}: {}, {}",
            uri, status, body
        ))));
    }
    Ok(body)
}

#[cfg(target_arch = "wasm32")]
async fn send_request_to_uri(uri: &str) -> MmResult<Json, GetInfoFromUriError> {
    use mm2_net::wasm_http::FetchRequest;

    macro_rules! try_or {
        ($exp:expr, $errtype:ident) => {
            match $exp {
                Ok(x) => x,
                Err(e) => return Err(MmError::new(GetInfoFromUriError::$errtype(ERRL!("{:?}", e)))),
            }
        };
    }

    let result = FetchRequest::get(uri)
        .header(ACCEPT.as_str(), APPLICATION_JSON)
        .request_str()
        .await;
    let (status_code, response_str) = try_or!(result, Transport);
    if !status_code.is_success() {
        return Err(MmError::new(GetInfoFromUriError::Transport(ERRL!(
            "!200: {}, {}",
            status_code,
            response_str
        ))));
    }

    let response: Json = try_or!(serde_json::from_str(&response_str), InvalidResponse);
    Ok(response)
}

async fn try_get_uri_meta(token_uri: &Option<String>) -> MmResult<UriMeta, GetNftInfoError> {
    match token_uri {
        Some(token_uri) => {
            if let Ok(response_meta) = send_request_to_uri(token_uri).await {
                let uri_meta_res: UriMeta = serde_json::from_str(&response_meta.to_string())?;
                Ok(uri_meta_res)
            } else {
                Ok(UriMeta::default())
            }
        },
        None => Ok(UriMeta::default()),
    }
}

/// `update_nft_list` function gets nft transfers from NFT HISTORY table, iterates through them
/// and updates NFT LIST table info.
async fn update_nft_list<T>(
    ctx: MmArc,
    storage: &T,
    chain: &Chain,
    scan_from_block: u32,
    url: &Url,
) -> MmResult<(), UpdateNftError>
where
    T: NftListStorageOps + NftTxHistoryStorageOps,
{
    let txs = storage.get_txs_from_block(chain, scan_from_block).await?;
    for tx in txs.into_iter() {
        let req = MyAddressReq {
            coin: chain.to_ticker(),
        };
        let my_address = get_my_address(ctx.clone(), req).await?.wallet_address.to_lowercase();
        match (tx.status, tx.contract_type) {
            (TransferStatus::Send, ContractType::Erc721) => {
                if let Some(nft) = storage
                    .get_nft(chain, tx.token_address.clone(), tx.token_id.clone())
                    .await?
                {
                    storage
                        .update_txs_meta_by_token_addr_id(
                            chain,
                            nft.token_address,
                            nft.token_id,
                            nft.collection_name,
                            nft.uri_meta.image,
                            nft.uri_meta.token_name,
                        )
                        .await?;
                } else {
                    continue;
                };
                storage
                    .remove_nft_from_list(chain, tx.token_address, tx.token_id, tx.block_number)
                    .await?;
            },
            (TransferStatus::Receive, ContractType::Erc721) => {
                let mut nft = get_moralis_metadata(tx.token_address, tx.token_id, chain, url).await?;
                // sometimes moralis updates Get All NFTs (which also affects Get Metadata) later
                // than History by Wallet update
                nft.owner_of = my_address;
                nft.block_number = tx.block_number;
                drop_mutability!(nft);
                storage
                    .add_nfts_to_list(chain, [nft.clone()], tx.block_number as u32)
                    .await?;
                storage
                    .update_txs_meta_by_token_addr_id(
                        chain,
                        nft.token_address,
                        nft.token_id,
                        nft.collection_name,
                        nft.uri_meta.image,
                        nft.uri_meta.token_name,
                    )
                    .await?;
            },
            (TransferStatus::Send, ContractType::Erc1155) => {
                let nft_db = storage
                    .get_nft(chain, tx.token_address.clone(), tx.token_id.clone())
                    .await?;
                // If nft exists then check the amount
                if let Some(mut nft_db) = nft_db {
                    match nft_db.amount.cmp(&tx.amount) {
                        Ordering::Equal => {
                            if let Some(nft) = storage
                                .get_nft(chain, tx.token_address.clone(), tx.token_id.clone())
                                .await?
                            {
                                storage
                                    .update_txs_meta_by_token_addr_id(
                                        chain,
                                        nft.token_address,
                                        nft.token_id,
                                        nft.collection_name,
                                        nft.uri_meta.image,
                                        nft.uri_meta.token_name,
                                    )
                                    .await?;
                            } else {
                                continue;
                            };
                            storage
                                .remove_nft_from_list(chain, tx.token_address, tx.token_id, tx.block_number)
                                .await?;
                        },
                        Ordering::Greater => {
                            nft_db.amount -= tx.amount;
                            drop_mutability!(nft_db);
                            storage.update_nft_amount(chain, nft_db, tx.block_number).await?;
                        },
                        Ordering::Less => {
                            return MmError::err(UpdateNftError::InsufficientAmountInCache {
                                amount_list: nft_db.amount.to_string(),
                                amount_history: tx.amount.to_string(),
                            });
                        },
                    }
                } else {
                    // token must exist in NFT LIST table
                    return MmError::err(UpdateNftError::TokenNotFoundInWallet {
                        token_address: tx.token_address,
                        token_id: tx.token_id.to_string(),
                    });
                }
            },
            (TransferStatus::Receive, ContractType::Erc1155) => {
                let nft_db = storage
                    .get_nft(chain, tx.token_address.clone(), tx.token_id.clone())
                    .await?;
                // If token isn't in NFT LIST table then add nft to the table.
                if let Some(mut nft_db) = nft_db {
                    // if owner address == from address, then owner sent tokens to themself,
                    // which means that the amount will not change.
                    if my_address != tx.from_address {
                        nft_db.amount += tx.amount;
                    }
                    nft_db.block_number = tx.block_number;
                    drop_mutability!(nft_db);
                    storage
                        .update_nft_amount_and_block_number(chain, nft_db.clone())
                        .await?;
                    storage
                        .update_txs_meta_by_token_addr_id(
                            chain,
                            nft_db.token_address,
                            nft_db.token_id,
                            nft_db.collection_name,
                            nft_db.uri_meta.image,
                            nft_db.uri_meta.token_name,
                        )
                        .await?;
                } else {
                    let moralis_meta = get_moralis_metadata(tx.token_address, tx.token_id.clone(), chain, url).await?;
                    let uri_meta = try_get_uri_meta(&moralis_meta.token_uri).await?;
                    let nft = Nft {
                        chain: *chain,
                        token_address: moralis_meta.token_address,
                        token_id: moralis_meta.token_id,
                        amount: tx.amount,
                        owner_of: my_address,
                        token_hash: moralis_meta.token_hash,
                        block_number_minted: moralis_meta.block_number_minted,
                        block_number: tx.block_number,
                        contract_type: moralis_meta.contract_type,
                        collection_name: moralis_meta.collection_name,
                        symbol: moralis_meta.symbol,
                        token_uri: moralis_meta.token_uri,
                        metadata: moralis_meta.metadata,
                        last_token_uri_sync: moralis_meta.last_token_uri_sync,
                        last_metadata_sync: moralis_meta.last_metadata_sync,
                        minter_address: moralis_meta.minter_address,
                        possible_spam: moralis_meta.possible_spam,
                        uri_meta,
                    };
                    storage
                        .add_nfts_to_list(chain, [nft.clone()], tx.block_number as u32)
                        .await?;
                    storage
                        .update_txs_meta_by_token_addr_id(
                            chain,
                            nft.token_address,
                            nft.token_id,
                            nft.collection_name,
                            nft.uri_meta.image,
                            nft.uri_meta.token_name,
                        )
                        .await?;
                }
            },
        }
    }
    Ok(())
}

/// `find_wallet_nft_amount` function returns NFT amount of cached NFT.
/// Note: in db **token_address** is kept in **lowercase**, because Moralis returns all addresses in lowercase.
pub(crate) async fn find_wallet_nft_amount(
    ctx: &MmArc,
    chain: &Chain,
    token_address: String,
    token_id: BigDecimal,
) -> MmResult<BigDecimal, GetNftInfoError> {
    let storage = NftStorageBuilder::new(ctx).build()?;
    if !NftListStorageOps::is_initialized(&storage, chain).await? {
        NftListStorageOps::init(&storage, chain).await?;
    }

    let nft_meta = storage
        .get_nft(chain, token_address.to_lowercase(), token_id.clone())
        .await?;

    let wallet_amount = match nft_meta {
        Some(nft) => nft.amount,
        None => {
            return MmError::err(GetNftInfoError::TokenNotFoundInWallet {
                token_address,
                token_id: token_id.to_string(),
            })
        },
    };
    Ok(wallet_amount)
}

async fn cache_nfts_from_moralis<T>(
    ctx: &MmArc,
    storage: &T,
    chain: &Chain,
    url: &Url,
) -> MmResult<Vec<Nft>, UpdateNftError>
where
    T: NftListStorageOps + NftTxHistoryStorageOps,
{
    let nft_list = get_moralis_nft_list(ctx, chain, url).await?;
    let last_scanned_block = NftTxHistoryStorageOps::get_last_block_number(storage, chain)
        .await?
        .unwrap_or(0);
    storage
        .add_nfts_to_list(chain, nft_list.clone(), last_scanned_block)
        .await?;
    Ok(nft_list)
}

async fn update_meta_in_txs<T>(storage: &T, chain: &Chain, nfts: Vec<Nft>) -> MmResult<(), UpdateNftError>
where
    T: NftListStorageOps + NftTxHistoryStorageOps,
{
    for nft in nfts.into_iter() {
        storage
            .update_txs_meta_by_token_addr_id(
                chain,
                nft.token_address,
                nft.token_id,
                nft.collection_name,
                nft.uri_meta.image,
                nft.uri_meta.token_name,
            )
            .await?;
    }
    Ok(())
}

async fn update_txs_with_empty_meta<T>(storage: &T, chain: &Chain, url: &Url) -> MmResult<(), UpdateNftError>
where
    T: NftListStorageOps + NftTxHistoryStorageOps,
{
    let nft_token_addr_id = storage.get_txs_with_empty_meta(chain).await?;
    for addr_id_pair in nft_token_addr_id.into_iter() {
        let nft_meta = get_moralis_metadata(addr_id_pair.token_address, addr_id_pair.token_id, chain, url).await?;
        storage
            .update_txs_meta_by_token_addr_id(
                chain,
                nft_meta.token_address,
                nft_meta.token_id,
                nft_meta.collection_name,
                nft_meta.uri_meta.image,
                nft_meta.uri_meta.token_name,
            )
            .await?;
    }
    Ok(())
}
