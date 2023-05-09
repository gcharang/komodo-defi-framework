use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::{MmError, MmResult};

pub(crate) mod nft_errors;
pub(crate) mod nft_structs;

use crate::{get_my_address, MyAddressReq, WithdrawError};
use nft_errors::{GetNftInfoError, UpdateNftError};
use nft_structs::{Chain, Nft, NftList, NftListReq, NftMetadataReq, NftTransferHistory, NftTransferHistoryWrapper,
                  NftTransfersReq, NftWrapper, NftsTransferHistoryList, TransactionNftDetails, UpdateNftReq,
                  WithdrawNftReq};

use crate::eth::{get_eth_address, withdraw_erc1155, withdraw_erc721};
use crate::nft::nft_structs::{ContractType, ConvertChain};
use crate::nft_storage::{NftListStorageOps, NftStorageBuilder, NftTxHistoryStorageOps};
use common::{APPLICATION_JSON, X_API_KEY};
use http::header::ACCEPT;
use mm2_number::BigDecimal;
use serde_json::Value as Json;

/// url for moralis requests
pub const URL_MORALIS: &str = "https://deep-index.moralis.io/api/v2/";
/// query parameter for moralis request: The format of the token ID
pub const FORMAT_DECIMAL_MORALIS: &str = "format=decimal";
/// query parameter for moralis request: The transfer direction
pub const DIRECTION_BOTH_MORALIS: &str = "direction=both";

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
        .get_nft(&req.chain, req.token_address.clone(), req.token_id.clone())
        .await?;
    if let Some(nft) = nft {
        Ok(nft)
    } else {
        MmError::err(GetNftInfoError::TokenNotFoundInWallet {
            token_address: req.token_address,
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
    let mut adds = Vec::new();
    for chain in req.chains.iter() {
        let req = MyAddressReq {
            coin: chain.to_ticker(),
        };
        let add = get_my_address(ctx.clone(), req).await?;
        adds.push(add.wallet_address.to_lowercase());
    }
    drop_mutability!(adds);
    let chain_addr: Vec<(Chain, String)> = req.chains.into_iter().zip(adds.into_iter()).collect();
    let transfer_history_list = storage
        .get_tx_history(chain_addr, req.max, req.limit, req.page_number, req.filters)
        .await?;
    Ok(transfer_history_list)
}

pub async fn update_nft(ctx: MmArc, req: UpdateNftReq) -> MmResult<(), UpdateNftError> {
    let storage = NftStorageBuilder::new(&ctx).build()?;
    for chain in req.chains.iter() {
        let tx_history_initialized = NftTxHistoryStorageOps::is_initialized(&storage, chain).await?;
        let list_initialized = NftListStorageOps::is_initialized(&storage, chain).await?;

        if !tx_history_initialized {
            NftTxHistoryStorageOps::init(&storage, chain).await?;
            let nft_transfers = get_moralis_nft_transfers(&ctx, chain, None).await?;
            storage.add_txs_to_history(chain, nft_transfers).await?;
        } else {
            let last_tx_block = NftTxHistoryStorageOps::get_last_block_number(&storage, chain).await?;
            let nft_transfers = get_moralis_nft_transfers(&ctx, chain, last_tx_block.map(|b| b + 1)).await?;
            storage.add_txs_to_history(chain, nft_transfers).await?;
        }

        if !list_initialized {
            NftListStorageOps::init(&storage, chain).await?;
            let nft_list = get_moralis_nft_list(&ctx, chain).await?;
            storage.add_nfts_to_list(chain, nft_list).await?;
        } else {
            let last_nft_block = NftListStorageOps::get_last_block_number(&storage, chain).await?;
            // check if last block number exists
            if let Some(last_block) = last_nft_block {
                // try to update nft list info using updated tx info from transfer history table.
                let txs = storage.get_txs_from_block(chain, last_block + 1).await?;
                for tx in txs.into_iter() {
                    let req = MyAddressReq {
                        coin: chain.to_ticker(),
                    };
                    let my_address = get_my_address(ctx.clone(), req).await?.wallet_address.to_lowercase();
                    match (tx.from_address == my_address, tx.contract_type) {
                        (true, ContractType::Erc721) => {
                            storage
                                .remove_nft_from_list(chain, tx.token_address, tx.token_id)
                                .await?;
                        },
                        (false, ContractType::Erc721) => {
                            let mut nft = get_moralis_metadata(&ctx, tx.token_address, tx.token_id, chain).await?;
                            // sometimes moralis updates Get All NFTs (which also affects Get Metadata) later
                            // than History by Wallet update
                            nft.owner_of = my_address;
                            nft.block_number = tx.block_number;
                            drop_mutability!(nft);
                            storage.add_nfts_to_list(chain, [nft]).await?;
                        },
                        (true, ContractType::Erc1155) => {
                            let nft_db = storage
                                .get_nft(chain, tx.token_address.clone(), tx.token_id.clone())
                                .await?;
                            // change amount or delete nft from the list
                            if let Some(mut nft_db) = nft_db {
                                match nft_db.amount.cmp(&tx.amount) {
                                    std::cmp::Ordering::Equal => {
                                        storage
                                            .remove_nft_from_list(chain, tx.token_address, tx.token_id)
                                            .await?;
                                    },
                                    std::cmp::Ordering::Greater => {
                                        nft_db.amount -= tx.amount;
                                        nft_db.block_number = tx.block_number;
                                        drop_mutability!(nft_db);
                                        storage.update_amount_block_number(chain, nft_db).await?;
                                    },
                                    std::cmp::Ordering::Less => {
                                        return MmError::err(UpdateNftError::InsufficientAmountInCache {
                                            amount_list: nft_db.amount.to_string(),
                                            amount_history: tx.amount.to_string(),
                                        });
                                    },
                                }
                            } else {
                                // if nft list table is not empty token must exist
                                return MmError::err(UpdateNftError::TokenNotFoundInWallet {
                                    token_address: tx.token_address,
                                    token_id: tx.token_id.to_string(),
                                });
                            }
                        },
                        (false, ContractType::Erc1155) => {
                            let nft_db = storage
                                .get_nft(chain, tx.token_address.clone(), tx.token_id.clone())
                                .await?;
                            // change amount or add nft to the list
                            if let Some(mut nft_db) = nft_db {
                                nft_db.amount += tx.amount;
                                nft_db.block_number = tx.block_number;
                                drop_mutability!(nft_db);
                                storage.update_amount_block_number(chain, nft_db).await?;
                            } else {
                                let moralis_meta =
                                    get_moralis_metadata(&ctx, tx.token_address, tx.token_id.clone(), chain).await?;
                                let nft = Nft {
                                    chain: *chain,
                                    token_address: moralis_meta.token_address,
                                    token_id: moralis_meta.token_id,
                                    amount: Default::default(),
                                    owner_of: my_address,
                                    token_hash: moralis_meta.token_hash,
                                    block_number_minted: moralis_meta.block_number_minted,
                                    block_number: tx.block_number,
                                    contract_type: moralis_meta.contract_type,
                                    name: moralis_meta.name,
                                    symbol: moralis_meta.symbol,
                                    token_uri: moralis_meta.token_uri,
                                    metadata: moralis_meta.metadata,
                                    last_token_uri_sync: moralis_meta.last_token_uri_sync,
                                    last_metadata_sync: moralis_meta.last_metadata_sync,
                                    minter_address: moralis_meta.minter_address,
                                    possible_spam: moralis_meta.possible_spam,
                                };
                                storage.add_nfts_to_list(chain, [nft]).await?;
                            }
                        },
                    }
                }
            } else {
                // if nft list table is empty, we can try to get info from moralis
                let nft_list = get_moralis_nft_list(&ctx, chain).await?;
                storage.add_nfts_to_list(chain, nft_list).await?;
            }
        }
    }
    Ok(())
}

pub async fn refresh_nft_metadata(ctx: MmArc, req: NftMetadataReq) -> MmResult<(), UpdateNftError> {
    let moralis_meta = get_moralis_metadata(&ctx, req.token_address.clone(), req.token_id.clone(), &req.chain).await?;
    let storage = NftStorageBuilder::new(&ctx).build()?;
    let req = NftMetadataReq {
        token_address: req.token_address,
        token_id: req.token_id,
        chain: req.chain,
    };
    let mut nft = get_nft_metadata(ctx, req).await?;
    nft.name = moralis_meta.name;
    nft.symbol = moralis_meta.symbol;
    nft.token_uri = moralis_meta.token_uri;
    nft.metadata = moralis_meta.metadata;
    nft.last_token_uri_sync = moralis_meta.last_token_uri_sync;
    nft.last_metadata_sync = moralis_meta.last_metadata_sync;
    nft.possible_spam = moralis_meta.possible_spam;
    drop_mutability!(nft);
    storage.refresh_nft_metadata(&moralis_meta.chain, nft).await?;
    Ok(())
}

/// `withdraw_nft` function generates, signs and returns a transaction that transfers NFT
/// from my address to recipient's address.
/// This method generates a raw transaction which should then be broadcast using `send_raw_transaction`.
pub async fn withdraw_nft(ctx: MmArc, req_type: WithdrawNftReq) -> WithdrawNftResult {
    match req_type {
        WithdrawNftReq::WithdrawErc1155(erc1155_req) => withdraw_erc1155(ctx, erc1155_req).await,
        WithdrawNftReq::WithdrawErc721(erc721_req) => withdraw_erc721(ctx, erc721_req).await,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn send_moralis_request(uri: &str, api_key: &str) -> MmResult<Json, GetNftInfoError> {
    use http::header::HeaderValue;
    use mm2_net::transport::slurp_req_body;

    let request = http::Request::builder()
        .method("GET")
        .uri(uri)
        .header(X_API_KEY, api_key)
        .header(ACCEPT, HeaderValue::from_static(APPLICATION_JSON))
        .body(hyper::Body::from(""))?;

    let (status, _header, body) = slurp_req_body(request).await?;
    if !status.is_success() {
        return Err(MmError::new(GetNftInfoError::Transport(format!(
            "Response !200 from {}: {}, {}",
            uri, status, body
        ))));
    }
    Ok(body)
}

#[cfg(target_arch = "wasm32")]
async fn send_moralis_request(uri: &str, api_key: &str) -> MmResult<Json, GetNftInfoError> {
    use mm2_net::wasm_http::FetchRequest;

    macro_rules! try_or {
        ($exp:expr, $errtype:ident) => {
            match $exp {
                Ok(x) => x,
                Err(e) => return Err(MmError::new(GetNftInfoError::$errtype(ERRL!("{:?}", e)))),
            }
        };
    }

    let result = FetchRequest::get(uri)
        .cors()
        .body_utf8("".to_owned())
        .header(X_API_KEY, api_key)
        .header(ACCEPT.as_str(), APPLICATION_JSON)
        .request_str()
        .await;
    let (status_code, response_str) = try_or!(result, Transport);
    if !status_code.is_success() {
        return Err(MmError::new(GetNftInfoError::Transport(ERRL!(
            "!200: {}, {}",
            status_code,
            response_str
        ))));
    }

    let response: Json = try_or!(serde_json::from_str(&response_str), InvalidResponse);
    Ok(response)
}

/// This function uses `get_nft_list` method to get the correct info about amount of specific NFT owned by my_address.
/// todo it is used for withdrawing, remove it and use db instead
pub(crate) async fn find_wallet_amount(
    ctx: MmArc,
    nft_list: NftListReq,
    token_address_req: String,
    token_id_req: BigDecimal,
) -> MmResult<BigDecimal, GetNftInfoError> {
    let nft_list = get_nft_list(ctx, nft_list).await?.nfts;
    let nft = nft_list
        .into_iter()
        .find(|nft| nft.token_address == token_address_req && nft.token_id == token_id_req)
        .ok_or_else(|| GetNftInfoError::TokenNotFoundInWallet {
            token_address: token_address_req,
            token_id: token_id_req.to_string(),
        })?;
    Ok(nft.amount)
}

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

async fn get_moralis_nft_transfers(
    ctx: &MmArc,
    chain: &Chain,
    from_block: Option<u32>,
) -> MmResult<Vec<NftTransferHistory>, GetNftInfoError> {
    let api_key = ctx.conf["api_key"]
        .as_str()
        .ok_or_else(|| MmError::new(GetNftInfoError::ApiKeyError))?;

    let mut res_list = Vec::new();
    let (coin_str, chain_str) = match chain {
        Chain::Avalanche => ("AVAX", "AVALANCHE"),
        Chain::Bsc => ("BNB", "BSC"),
        Chain::Eth => ("ETH", "ETH"),
        Chain::Fantom => ("FTM", "FANTOM"),
        Chain::Polygon => ("MATIC", "POLYGON"),
    };
    let my_address = get_eth_address(ctx, coin_str).await?;
    let from_block = match from_block {
        Some(block) => {
            format!("&from_block={}", block)
        },
        None => "".into(),
    };
    let uri_without_cursor = format!(
        "{}{}/nft/transfers?chain={}&{}&{}{}",
        URL_MORALIS, my_address.wallet_address, chain_str, FORMAT_DECIMAL_MORALIS, DIRECTION_BOTH_MORALIS, from_block
    );

    // The cursor returned in the previous response (used for getting the next page).
    let mut cursor = String::new();
    loop {
        let uri = format!("{}{}", uri_without_cursor, cursor);
        let response = send_moralis_request(uri.as_str(), api_key).await?;
        if let Some(transfer_list) = response["result"].as_array() {
            for transfer in transfer_list {
                let transfer_wrapper: NftTransferHistoryWrapper = serde_json::from_str(&transfer.to_string())?;
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
                    from_address: transfer_wrapper.from_address,
                    to_address: transfer_wrapper.to_address,
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
    ctx: &MmArc,
    token_address: String,
    token_id: BigDecimal,
    chain: &Chain,
) -> MmResult<Nft, GetNftInfoError> {
    let api_key = ctx.conf["api_key"]
        .as_str()
        .ok_or_else(|| MmError::new(GetNftInfoError::ApiKeyError))?;
    let chain_str = match chain {
        Chain::Avalanche => "AVALANCHE",
        Chain::Bsc => "BSC",
        Chain::Eth => "ETH",
        Chain::Fantom => "FANTOM",
        Chain::Polygon => "POLYGON",
    };
    let uri = format!(
        "{}nft/{}/{}?chain={}&{}",
        URL_MORALIS, token_address, token_id, chain_str, FORMAT_DECIMAL_MORALIS
    );
    let response = send_moralis_request(uri.as_str(), api_key).await?;
    let nft_wrapper: NftWrapper = serde_json::from_str(&response.to_string())?;
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
        name: nft_wrapper.name,
        symbol: nft_wrapper.symbol,
        token_uri: nft_wrapper.token_uri,
        metadata: nft_wrapper.metadata,
        last_token_uri_sync: nft_wrapper.last_token_uri_sync,
        last_metadata_sync: nft_wrapper.last_metadata_sync,
        minter_address: nft_wrapper.minter_address,
        possible_spam: nft_wrapper.possible_spam,
    };
    Ok(nft_metadata)
}
