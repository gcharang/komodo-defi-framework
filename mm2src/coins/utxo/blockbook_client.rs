use crate::utxo::rpc_clients::{BlockHashOrHeight, EstimateFeeMethod, EstimateFeeMode, JsonRpcPendingRequestsShared,
                               SpentOutputInfo, UnspentInfo, UnspentMap, UtxoJsonRpcClientInfo, UtxoRpcClientOps,
                               UtxoRpcError, UtxoRpcFut};
use crate::utxo::utxo_block_header_storage::BlockHeaderStorage;
use crate::utxo::{GetBlockHeaderError, NonZeroU64};
use crate::{RpcTransportEventHandler, RpcTransportEventHandlerShared};
use async_trait::async_trait;
use chain::{TransactionInput, TransactionOutput};
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
use rpc::v1::types::{Bytes, H256};
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

impl BlockBookClient {
    pub async fn block_hash(&self, height: u64) -> BlockBookResult<H256> { todo!() }
    pub async fn transaction(&self, txid: &str) -> BlockBookResult<Json> {
        let tx = self.query(format!("/api/v2/tx/{txid}")).await?;
        Ok(tx)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct Transaction {
    pub txid: String,
    pub version: u8,
    pub lock_time: Option<u32>,
    pub size: u32,
    pub vsize: u32,
    /// `None` for unconfirmed transactions
    pub block_hash: Option<u64>,
    /// `None` for unconfirmed transactions
    pub block_height: Option<u64>,
    pub confirmations: u32,
    pub block_time: u32,
    pub value: u32,
    pub value_in: u32,
    pub fees: u32,
}

#[derive(Debug, Display)]
pub enum BlockBookClientError {
    Transport(String),
    #[display(fmt = "'{}' asset is not yet supported by blockbook", coin)]
    NotSupported {
        coin: String,
    },
}

#[test]
fn test_get_tickers() {
    let blockbook = BlockBookClient::new("BTC").unwrap();

    let get = block_on(blockbook.transaction("bdb31013359ff66978e7a8bba987ba718a556c85c4051ddb1e83b1b36860734b"));

    println!("{:#?}", get);
}

//Ok(
//Object({
//"txid": String(
//"bdb31013359ff66978e7a8bba987ba718a556c85c4051ddb1e83b1b36860734b",
//),
//"version": Number(
//1,
//),
//"vin": Array([
//Object({
//"sequence": Number(
//4294967295,
//),
//"n": Number(
//0,
//),
//"isAddress": Bool(
//false,
//),
//"coinbase": String(
//"0320e20b1362696e616e63652f383037b200300023939608fabe6d6d55f6a0dbe69b17a1e50d896b26101066e30e1ae2545fde72e473578639a6db1404000000000000000000cd86328e1d0000000000",
//),
//}),
//]),
//"vout": Array([
//Object({
//"value": String(
//"633568466",
//),
//"n": Number(
//0,
//),
//"spent": Bool(
//true,
//),
//"spentTxId": String(
//"5f9aae3398190cd6b64cf10d5c1a87dc2fbd5f68b7c05179ab1cf8709e614817",
//),
//"spentIndex": Number(
//18,
//),
//"spentHeight": Number(
//778917,
//),
//"hex": String(
//"a914ca35b1f4d02907314852f09935b9604507f8d70087",
//),
//"addresses": Array([
//String(
//"3L8Ck6bm3sve1vJGKo6Ht2k167YKSKi8TZ",
//),
//]),
//"isAddress": Bool(
//true,
//),
//}),
//Object({
//"value": String(
//"0",
//),
//"n": Number(
//1,
//),
//"hex": String(
//"6a24aa21a9eddd6768bc163d82de1312c2bca5c78622fce02645fbc8f271b11618d467296775",
//),
//"addresses": Array([
//String(
//"OP_RETURN aa21a9eddd6768bc163d82de1312c2bca5c78622fce02645fbc8f271b11618d467296775",
//),
//]),
//"isAddress": Bool(
//false,
//),
//}),
//Object({
//"value": String(
//"0",
//),
//"n": Number(
//2,
//),
//"hex": String(
//"6a2952534b424c4f434b3a43759aa8954c419c01904fdb84364b28f1544cd91f9ec4b15a277524004db85f",
//),
//"addresses": Array([
//String(
//"OP_RETURN 52534b424c4f434b3a43759aa8954c419c01904fdb84364b28f1544cd91f9ec4b15a277524004db85f",
//),
//]),
//"isAddress": Bool(
//false,
//),
//}),
//]),
//"blockHash": String(
//"00000000000000000003bde76adcbc25d0be3020ab630336cabb4ccf75c18555",
//),
//"blockHeight": Number(
//778784,
//),
//"confirmations": Number(
//17405,
//),
//"blockTime": Number(
//1677663144,
//),
//"size": Number(
//298,
//),
//"vsize": Number(
//271,
//),
//"value": String(
//"633568466",
//),
//"valueIn": String(
//"0",
//),
//"fees": String(
//"0",
//),
//"hex": String(
//"010000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff500320e20b1362696e616e63652f383037b200300023939608fabe6d6d55f6a0dbe69b17a1e50d896b26101066e30e1ae2545fde72e473578639a6db1404000000000000000000cd86328e1d0000000000ffffffff03d27cc3250000000017a914ca35b1f4d02907314852f09935b9604507f8d700870000000000000000266a24aa21a9eddd6768bc163d82de1312c2bca5c78622fce02645fbc8f271b11618d46729677500000000000000002b6a2952534b424c4f434b3a43759aa8954c419c01904fdb84364b28f1544cd91f9ec4b15a277524004db85f0120000000000000000000000000000000000000000000000000000000000000000000000000",
//),
//}),
//)
