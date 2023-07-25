#[path = "version2/wallet.rs"] pub mod wallet;

pub use wallet::{GetPublicKeyHashResponse, GetPublicKeyResponse, GetRawTransactionRequest, GetRawTransactionResponse};

use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use mm2_number::{MmNumber, MmNumberMultiRepr};

use super::legacy::OrderConfirmationsSettings;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MmRpcRequest<M, T> {
    pub mmrpc: MmRpcVersion,
    pub userpass: Option<String>,
    pub method: M,
    pub params: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<usize>,
}

#[derive(Clone, Deserialize, Serialize)]
pub enum MmRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MmRpcErrorV2 {
    pub error: String,
    pub error_path: String,
    pub error_trace: String,
    pub error_type: String,
    pub error_data: String,
}

#[derive(Deserialize)]
pub struct MmRpcResponseV2<T> {
    pub mmrpc: MmRpcVersion,
    #[serde(flatten)]
    pub result: MmRpcResultV2<T>,
    pub id: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum MmRpcResultV2<T> {
    Ok { result: T },
    Err(MmRpcErrorV2),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BestOrdersRequestV2 {
    pub coin: String,
    pub action: BestOrdersAction,
    pub request_by: BestOrdersByRequest,
    #[serde(default)]
    pub exclude_mine: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
#[serde(rename_all = "lowercase")]
pub enum BestOrdersByRequest {
    Volume(MmNumber),
    Number(usize),
}

#[derive(Clone, Debug, Deserialize, Display, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BestOrdersAction {
    Buy,
    Sell,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcOrderbookEntryV2 {
    pub uuid: Uuid,
    pub coin: String,
    pub address: OrderbookAddress,
    pub price: MmNumberMultiRepr,
    pub pubkey: String,
    pub is_mine: bool,
    pub base_max_volume: MmNumberMultiRepr,
    pub base_min_volume: MmNumberMultiRepr,
    pub rel_max_volume: MmNumberMultiRepr,
    pub rel_min_volume: MmNumberMultiRepr,
    pub conf_settings: Option<OrderConfirmationsSettings>,
}

#[derive(Deserialize, Serialize)]
pub struct BestOrdersV2Response {
    pub orders: HashMap<String, Vec<RpcOrderbookEntryV2>>,
    pub original_tickers: HashMap<String, HashSet<String>>,
}

#[derive(Clone, Debug, Deserialize, Display, Serialize)]
#[serde(tag = "address_type", content = "address_data")]
pub enum OrderbookAddress {
    Transparent(String),
    Shielded,
}
