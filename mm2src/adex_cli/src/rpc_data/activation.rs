//! Contains rpc data layer structures that are not ready to become a part of the mm2_rpc::data module
//!
//! *Note: it's expected that the following data types will be moved to mm2_rpc::data when mm2 is refactored to be able to handle them*
//!

#[path = "activation/bch.rs"] pub mod bch;
#[path = "activation/electrum.rs"] mod electrum;
#[path = "activation/enable.rs"] mod enable;

use mm2_rpc::data::legacy::Mm2RpcResult;
use mm2_rpc::data::version2::MmRpcRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "get_enabled_coins")]
pub struct GetEnabledRequest {}

pub(crate) enum ActivationMethod {
    Legacy(ActivationRequestLegacy),
    V2(MmRpcRequest<V2ActivationMethod, ActivationV2Params>),
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum V2ActivationMethod {
    EnableBchWithTokens,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub(crate) enum ActivationRequestLegacy {
    Enable(enable::EnableRequest),
    Electrum(electrum::ElectrumRequest),
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum ActivationV2Params {
    BchWithTokensActivationRequest(bch::BchWithTokensActivationParams),
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

#[derive(Deserialize)]
pub(crate) struct SetRequiredConfResponse {
    pub(crate) coin: String,
    pub(crate) confirmations: u64,
}

#[derive(Deserialize)]
pub(crate) struct SetRequiredNotaResponse {
    pub(crate) coin: String,
    pub(crate) requires_notarization: bool,
}

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "coins_needed_for_kick_start")]
pub(crate) struct CoinsToKickStartRequest {}

pub(crate) type CoinsToKickstartResponse = Vec<String>;
