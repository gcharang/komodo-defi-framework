use serde::{Deserialize, Serialize};

use common::one_thousand_u32;

use crate::rpc_data::activation::ElectrumRpcRequest;

#[derive(Deserialize, Serialize)]
pub(crate) struct ZcoinActivationParams {
    pub(crate) mode: ZcoinRpcMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) required_confirmations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) requires_notarization: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) zcash_params_path: Option<String>,
    #[serde(default = "one_thousand_u32")]
    pub(crate) scan_blocks_per_iteration: u32,
    #[serde(default)]
    pub(crate) scan_interval_ms: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "rpc", content = "rpc_data")]
pub(crate) enum ZcoinRpcMode {
    #[cfg(not(target_arch = "wasm32"))]
    Native,
    Light {
        electrum_servers: Vec<ElectrumRpcRequest>,
        light_wallet_d_servers: Vec<String>,
    },
}
