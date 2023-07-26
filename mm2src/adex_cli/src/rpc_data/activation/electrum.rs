use mm2_rpc::data::legacy::{ElectrumProtocol, UtxoMergeParams};
use serde::{Deserialize, Serialize};

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
