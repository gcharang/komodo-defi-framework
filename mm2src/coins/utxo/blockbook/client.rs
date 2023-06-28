use crate::utxo::blockbook::structures::{BlockBookAddress, BlockBookBlock, BlockBookTickers, BlockBookTickersList,
                                         BlockBookTransaction, BlockBookTransactionSpecific, BlockBookUtxo,
                                         XpubTransactions};
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

const BTC_BLOCKBOOK_ENPOINT: &str = "https://btc1.trezor.io";
const ETH_BLOCKBOOK_ENPOINT: &str = "https://eth1.trezor.io";

pub type BlockBookResult<T> = MmResult<T, BlockBookClientError>;

#[derive(Debug)]
pub struct BlockBookClientImpl {
    pub endpoint: String,
    pub ticker: String,
}

#[derive(Clone, Debug)]
pub struct BlockBookClient(pub Arc<BlockBookClientImpl>);

impl BlockBookClient {
    pub fn new(ticker: &str) -> BlockBookResult<Self> {
        let endpoint = match ticker {
            BTC => BTC_BLOCKBOOK_ENPOINT,
            ETH => ETH_BLOCKBOOK_ENPOINT,
            _ => {
                return Err(MmError::new(BlockBookClientError::NotSupported {
                    coin: ticker.to_string(),
                }))
            },
        };

        let client = BlockBookClientImpl {
            endpoint: endpoint.to_string(),
            ticker: ticker.to_string(),
        };

        Ok(Self(Arc::new(client)))
    }

    pub async fn query(&self, path: String) -> BlockBookResult<Json> {
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
trait BlockBookClientClientOps {
    async fn block_hash(&self, height: u64) -> BlockBookResult<H256>;
    async fn transaction(&self, txid: &str) -> BlockBookResult<BlockBookTransaction> {
        let tx = self.query(format!("/api/v2/tx/{txid}")).await?;
        let json = serde_json::from_value::<BlockBookTransaction>(tx).unwrap();
        Ok(json)
    }
    async fn transaction_specific(&self, txid: &str) -> BlockBookResult<BlockBookTransactionSpecific>;
    async fn address(&self, address: &str) -> BlockBookResult<BlockBookAddress>;
    async fn xpub(&self, address: &str) -> BlockBookResult<XpubTransactions>;
    async fn utxo(&self, address: &str) -> BlockBookResult<Vec<BlockBookUtxo>>;
    async fn block(&self, address: &str) -> BlockBookResult<BlockBookBlock>;
    async fn send_transaction(&self, address: &str) -> BlockBookResult<H256>;
    async fn tickers_list(&self, address: &str) -> BlockBookResult<BlockBookTickersList>;
    async fn tickers(&self, address: &str) -> BlockBookResult<BlockBookTickers>;
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
