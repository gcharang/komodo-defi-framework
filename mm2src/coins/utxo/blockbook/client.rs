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
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use tonic::IntoRequest;

// const BTC_BLOCKBOOK_ENPOINT: &str = "https://btc1.trezor.io";
// const ETH_BLOCKBOOK_ENPOINT: &str = "https://eth1.trezor.io";

pub type BlockBookResult<T> = MmResult<T, BlockBookClientError>;

pub struct BlockBookClientImpl {
    pub endpoint: String,
    pub chain: Box<dyn BlockBookClientOps>,
}

#[derive(Clone)]
pub struct BlockBookClient(pub Arc<BlockBookClientImpl>);

impl BlockBookClient {
    pub fn new(endpoint: &str, chain: impl BlockBookClientOps) -> BlockBookResult<Self> {
        Ok(Self(Arc::new(BlockBookClientImpl {
            endpoint: endpoint.to_string(),
            chain: Box::new((chain)),
        })))
    }
}

#[async_trait]
pub trait BlockBookClientTransport: Send + Sync + 'static {
    async fn query(&self, path: String) -> BlockBookResult<Json> {
        use http::header::HeaderValue;

        let uri = format!("{}{path}", self.0.endpoint);
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
}

#[async_trait]
pub trait BlockBookClientOps: BlockBookClientTransport + Send + Sync + 'static {
    /// Status page returns current status of Blockbook and connected backend.
    async fn status(&self, height: u64) -> BlockBookResult<H256>;

    /// Get current block of a given height.
    async fn block_hash(&self, height: u64) -> BlockBookResult<H256>;

    /// Get transaction returns "normalized" data about transaction, which has the same general structure for all
    /// supported coins. It does not return coin specific fields.
    async fn get_transaction(&self, txid: &str) -> BlockBookResult<BlockBookTransaction>;

    /// Get transaction data in the exact format as returned by backend, including all coin specific fields:
    async fn get_transaction_specific(&self, txid: &str) -> BlockBookResult<BlockBookTransactionSpecific>;

    /// Get balances and transactions of an address. The returned transactions are sorted by block height, newest
    /// blocks first. see `[strutures::GetAddressParams]` for extra query arguments.
    async fn address(&self, address: &str, query_params: Option<GetAddressParams>)
        -> BlockBookResult<BlockBookAddress>;

    /// Get balances and transactions of an xpub or output descriptor, applicable only for Bitcoin-type coins. see
    /// `[strutures::GetAddressParams]` for extra query arguments.
    async fn xpub(&self, xpub: &str, query_params: Option<GetAddressParams>) -> BlockBookResult<XpubTransactions>;

    // Get array of unspent transaction outputs of address or xpub, applicable only for Bitcoin-type coins. By default,
    // the list contains both confirmed and unconfirmed transactions. The query parameter confirmed=true disables return of unconfirmed transactions. The returned utxos are sorted by block height, newest blocks first. For xpubs or output descriptors, the response also contains address and derivation path of the utxo.
    async fn utxo(&self, address: &str, confirmed: bool) -> BlockBookResult<Vec<BlockBookUtxo>>;

    /// Get information about block with transactions, either by height or hash
    async fn block(&self, block_by: &GetBlockByHashHeight) -> BlockBookResult<BlockBookBlock>;

    /// Sends new transaction to backend.
    async fn send_transaction(&self, hex: &RawTransaction) -> BlockBookResult<H256>;

    /// Get a list of available currency rate tickers (secondary currencies) for the specified date, along with an
    /// actual data timestamp.
    async fn tickers_list(&self, timestamp: Option<u32>) -> BlockBookResult<BlockBookTickersList>;

    /// Get currency rate for the specified currency and date. If the currency is not available for that specific
    /// timestamp, the next closest rate will be returned. All responses contain an actual rate timestamp.
    async fn tickers(&self, currency: Option<&str>, timestamp: Option<u32>) -> BlockBookResult<BlockBookTickers>;

    /// Returns a balance history for the specified XPUB or address.
    async fn balance_history(
        &self,
        address: &str,
        query_params: BalanceHistoryParams,
    ) -> BlockBookResult<BlockBookBalanceHistory>;
}

#[derive(Debug, Display)]
pub enum BlockBookClientError {
    Transport(String),
    #[display(fmt = "'{}' asset is not yet supported by blockbook", coin)]
    NotSupported {
        coin: String,
    },
}

//#[test]
//fn test_get_tickers() {
//    let blockbook = BlockBookClient::new("BTC").unwrap();
//
//    let get = block_on(blockbook.transaction("bdb31013359ff66978e7a8bba987ba718a556c85c4051ddb1e83b1b36860734b"));
//
//    println!("{:?}", get);
//}
