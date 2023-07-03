use crate::utxo::blockbook::structures::{BalanceHistoryParams, BlockBookAddress, BlockBookBalanceHistory,
                                         BlockBookBlock, BlockBookTickers, BlockBookTickersList, BlockBookTransaction,
                                         BlockBookTransactionSpecific, BlockBookUtxo, GetAddressParams,
                                         GetBlockByHashHeight, XpubTransactions};
use crate::utxo::rpc_clients::{BlockHashOrHeight, EstimateFeeMethod, EstimateFeeMode, JsonRpcPendingRequestsShared,
                               SpentOutputInfo, UnspentInfo, UnspentMap, UtxoJsonRpcClientInfo, UtxoRpcClientOps,
                               UtxoRpcError, UtxoRpcFut};
use crate::utxo::utxo_block_header_storage::BlockHeaderStorage;
use crate::utxo::{GetBlockHeaderError, NonZeroU64};
use crate::{RpcTransportEventHandler, RpcTransportEventHandlerShared};
use async_trait::async_trait;
use bitcoin::Amount;
use bitcoin::Denomination::Satoshi;
use chain::TransactionInput;
use common::executor::abortable_queue::AbortableQueue;
use common::jsonrpc_client::{JsonRpcClient, JsonRpcErrorType, JsonRpcRemoteAddr, JsonRpcRequest, JsonRpcRequestEnum,
                             JsonRpcResponseEnum, JsonRpcResponseFut, RpcRes};
use common::{block_on, APPLICATION_JSON};
use futures::FutureExt;
use futures::TryFutureExt;
use futures01::sync::mpsc;
use futures01::Future;
use http::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use http::{Request, StatusCode};
use keys::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::MmError;
use mm2_err_handle::prelude::{MapToMmFutureExt, MapToMmResult, MmResult};
use mm2_net::transport::slurp_req_body;
use mm2_number::BigDecimal;
use rpc::v1::types::{deserialize_null_default, Bytes, RawTransaction, SignedTransactionOutput, TransactionInputEnum,
                     TransactionOutputScript, H256};
use serde::{Deserialize, Deserializer};
use serde_json::{self as json, Value as Json};
use serialization::CoinVariant;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use tonic::IntoRequest;

const BTC_BLOCKBOOK_ENPOINT: &str = "https://btc1.trezor.io";

pub type BlockBookResult<T> = MmResult<T, BlockBookClientError>;

#[derive(Debug)]
pub struct BlockBookClientImpl {
    pub ticker: String,
    pub url: String,
}

#[derive(Debug)]
pub struct BlockBookClient(pub Arc<BlockBookClientImpl>);

impl BlockBookClient {
    pub fn new(url: &str, ticker: &str) -> Self {
        Self(Arc::new(BlockBookClientImpl {
            url: url.to_string(),
            ticker: ticker.to_string(),
        }))
    }

    pub async fn query(&self, path: String) -> BlockBookResult<Json> {
        use http::header::HeaderValue;

        let uri = format!("{}{path}", self.0.url);
        let request = http::Request::builder()
            .method("GET")
            .uri(uri.clone())
            .header(ACCEPT, HeaderValue::from_static(APPLICATION_JSON))
            .header(
                USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:90.0) Gecko/20100101 Firefox/90.0",
            )
            .body(hyper::Body::from(""))
            .map_err(|err| BlockBookClientError::Transport(err.to_string()))?;

        let (status, _header, body) = slurp_req_body(request)
            .await
            .map_err(|err| MmError::new(BlockBookClientError::Transport(err.to_string())))?;

        if !status.is_success() {
            return Err(MmError::new(BlockBookClientError::Transport(format!(
                "Response !200 from {}: {}, {}",
                uri, status, body
            ))));
        }

        Ok(body)
    }

    /// Status page returns current status of Blockbook and connected backend.
    async fn status(&self, _height: u64) -> BlockBookResult<H256> { todo!() }

    /// Get current block of a given height.
    async fn block_hash(&self, height: u64) -> BlockBookResult<H256> {
        let path = format!("/api/v2/block-index/{height}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res["blockHash"].clone())
            .map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get transaction returns "normalized" data about transaction, which has the same general structure for all
    /// supported coins. It does not return coin specific fields.
    async fn get_transaction(&self, _txid: &str) -> BlockBookResult<BlockBookTransaction> { todo!() }

    /// Get transaction data in the exact format as returned by backend, including all coin specific fields:
    async fn get_transaction_specific(&self, _txid: &str) -> BlockBookResult<BlockBookTransactionSpecific> { todo!() }

    /// Get balances and transactions of an address. The returned transactions are sorted by block height, newest
    /// blocks first. see `[strutures::GetAddressParams]` for extra query arguments.
    async fn address(
        &self,
        _address: &str,
        _query_params: Option<GetAddressParams>,
    ) -> BlockBookResult<BlockBookAddress> {
        todo!()
    }

    /// Get balances and transactions of an xpub or output descriptor, applicable only for Bitcoin-type coins. see
    /// `[strutures::GetAddressParams]` for extra query arguments.
    async fn xpub(&self, _xpub: &str, _query_params: Option<GetAddressParams>) -> BlockBookResult<XpubTransactions> {
        todo!()
    }

    // Get array of unspent transaction outputs of address or xpub, applicable only for Bitcoin-type coins. By default,
    // the list contains both confirmed and unconfirmed transactions. The query parameter confirmed=true disables return of unconfirmed transactions. The returned utxos are sorted by block height, newest blocks first. For xpubs or output descriptors, the response also contains address and derivation path of the utxo.
    async fn utxo(&self, _address: &str, _confirmed: bool) -> BlockBookResult<Vec<BlockBookUtxo>> { todo!() }

    /// Get information about block with transactions, either by height or hash
    async fn block(&self, _block_by: &GetBlockByHashHeight) -> BlockBookResult<BlockBookBlock> { todo!() }

    /// Sends new transaction to backend.
    async fn send_transaction(&self, _hex: &RawTransaction) -> BlockBookResult<H256> { todo!() }

    /// Get a list of available currency rate tickers (secondary currencies) for the specified date, along with an
    /// actual data timestamp.
    async fn tickers_list(&self, _timestamp: Option<u32>) -> BlockBookResult<BlockBookTickersList> { todo!() }

    /// Get currency rate for the specified currency and date. If the currency is not available for that specific
    /// timestamp, the next closest rate will be returned. All responses contain an actual rate timestamp.
    async fn tickers(&self, _currency: Option<&str>, _timestamp: Option<u32>) -> BlockBookResult<BlockBookTickers> {
        todo!()
    }

    /// Returns a balance history for the specified XPUB or address.
    async fn balance_history(
        &self,
        _address: &str,
        _query_params: BalanceHistoryParams,
    ) -> BlockBookResult<BlockBookBalanceHistory> {
        todo!()
    }
}

#[derive(Debug, Display)]
pub enum BlockBookClientError {
    Transport(String),
    ResponseError(String),
    #[display(fmt = "'{}' asset is not yet supported by blockbook", coin)]
    NotSupported {
        coin: String,
    },
}

#[test]
fn test_block_hash() {
    let blockbook = BlockBookClient::new(BTC_BLOCKBOOK_ENPOINT, "BTC");
    let block_hash = block_on(blockbook.block_hash(0)).unwrap();
    let genesis_block = H256::from_str("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f").unwrap();

    assert_eq!(genesis_block, block_hash);
}
