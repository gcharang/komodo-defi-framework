//! Contains rpc data layer structures that are not ready to become a part of the mm2_rpc::data module
//!
//! *Note: it's expected that the following data types will be moved to mm2_rpc::data when mm2 is refactored to be able to handle them*
//!

#[path = "activation/bch.rs"] pub(crate) mod bch;
#[path = "activation/electrum.rs"] mod electrum;
#[path = "activation/enable.rs"] mod enable;
#[path = "activation/eth.rs"] pub(crate) mod eth;
#[path = "activation/tendermint.rs"] pub(crate) mod tendermint;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use derive_more::Display;
use mm2_number::BigDecimal;
use mm2_rpc::data::legacy::Mm2RpcResult;

use crate::rpc_data::eth::EthWithTokensActivationRequest;

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "get_enabled_coins")]
pub struct GetEnabledRequest {}

pub(crate) enum ActivationMethod {
    Legacy(ActivationMethodLegacy),
    V2(ActivationMethodV2),
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub(crate) enum ActivationMethodLegacy {
    Enable(enable::EnableRequest),
    Electrum(electrum::ElectrumRequest),
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub(crate) enum ActivationMethodV2 {
    EnableBchWithTokens(EnablePlatformCoinWithTokensReq<bch::BchWithTokensActivationParams>),
    EnableSlp(EnableTokenRequest<bch::SlpActivationRequest>),
    EnableTendermintWithAssets(EnablePlatformCoinWithTokensReq<tendermint::TendermintActivationParams>),
    EnableTendermintToken(EnableTokenRequest<tendermint::TendermintTokenActivationParams>),
    EnableEthWithTokens(EnablePlatformCoinWithTokensReq<EthWithTokensActivationRequest>),
    EnableErc20(EnableTokenRequest<eth::Erc20TokenActivationRequest>),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EnablePlatformCoinWithTokensReq<T: Serialize> {
    ticker: String,
    #[serde(flatten)]
    request: T,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TokenActivationRequest<Req> {
    ticker: String,
    #[serde(flatten)]
    request: Req,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EnableTokenRequest<T> {
    ticker: String,
    activation_params: T,
}

#[derive(Default, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CoinBalance {
    pub(crate) spendable: BigDecimal,
    pub(crate) unspendable: BigDecimal,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CoinAddressInfo<Balance> {
    pub(crate) derivation_method: DerivationMethod,
    pub(crate) pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) balances: Option<Balance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tickers: Option<HashSet<String>>,
}

pub(crate) type TokenBalances = HashMap<String, CoinBalance>;

#[derive(Display, Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum DerivationMethod {
    Iguana,
    #[allow(dead_code)]
    HDWallet(String),
}
