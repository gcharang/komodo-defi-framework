use mm2_rpc::data::legacy::GasStationPricePolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EnableRequest {
    coin: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    urls: Vec<String>,
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
