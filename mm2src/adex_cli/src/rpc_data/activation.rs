//! Contains rpc data layer structures that are not ready to become a part of the mm2_rpc::data module
//!
//! *Note: it's expected that the following data types will be moved to mm2_rpc::data when mm2 is refactored to be able to handle them*
//!

use mm2_rpc::data::legacy::{ElectrumProtocol, GasStationPricePolicy, Mm2RpcResult, UtxoMergeParams};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};
use uuid::Uuid;

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "get_enabled_coins")]
pub struct GetEnabledRequest {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub(crate) enum ActivationRequest {
    Enable(EnableRequest),
    Electrum(ElectrumRequest),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EnableRequest {
    coin: String,
    #[serde(default, serialize_with = "serialize_urls", skip_serializing_if = "Vec::is_empty")]
    urls: Vec<EnableUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    swap_contract_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_swap_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gas_station_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gas_station_decimals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gas_station_policy: Option<GasStationPricePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mm2: Option<u8>,
    #[serde(default)]
    tx_history: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_confirmations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_notarization: Option<bool>,
    #[serde(default)]
    contract_supports_watchers: Option<bool>,
}

fn serialize_urls<S>(urls: &Vec<EnableUrl>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut s_seq = s.serialize_seq(None)?;
    for url in urls {
        s_seq.serialize_element(url.url.as_str())?;
    }
    s_seq.end()
}

#[derive(Debug, Deserialize)]
struct EnableUrl {
    url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ElectrumRequest {
    coin: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) servers: Vec<Server>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mm2: Option<u8>,
    #[serde(default)]
    tx_history: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_confirmations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_notarization: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    swap_contract_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_swap_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    utxo_merge_params: Option<UtxoMergeParams>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Server {
    url: String,
    #[serde(default)]
    protocol: ElectrumProtocol,
    #[serde(default)]
    disable_cert_verification: bool,
}

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "disable_coin")]
pub(crate) struct DisableCoinRequest {
    pub(crate) coin: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum DisableCoinResponse {
    Success(Mm2RpcResult<DisableCoinSuccess>),
    Failed(DisableCoinFailed),
}

#[derive(Deserialize)]
pub(crate) struct DisableCoinSuccess {
    pub(crate) coin: String,
    pub(crate) cancelled_orders: Vec<Uuid>,
    pub(crate) passivized: bool,
}

#[derive(Deserialize)]
pub(crate) struct DisableCoinFailed {
    pub(crate) error: String,
    pub(crate) orders: DisableFailedOrders,
    pub(crate) active_swaps: Vec<Uuid>,
}

#[derive(Deserialize)]
pub(crate) struct DisableFailedOrders {
    pub(crate) matching: Vec<Uuid>,
    pub(crate) cancelled: Vec<Uuid>,
}
