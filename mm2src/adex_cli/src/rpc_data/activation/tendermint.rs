use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use common::true_f;

use super::{CoinBalance, TokenActivationRequest};

#[derive(Deserialize, Serialize)]
pub(crate) struct TendermintActivationParams {
    rpc_urls: Vec<String>,
    tokens_params: Vec<TokenActivationRequest<TendermintTokenActivationParams>>,
    #[serde(default)]
    tx_history: bool,
    #[serde(default = "true_f")]
    get_balances: bool,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct TendermintTokenActivationParams {}

#[derive(Deserialize, Serialize)]
pub(crate) struct TendermintActivationResult {
    pub(crate) ticker: String,
    pub(crate) address: String,
    pub(crate) current_block: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) balance: Option<CoinBalance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tokens_balances: Option<HashMap<String, CoinBalance>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tokens_tickers: Option<HashSet<String>>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct TendermintTokenInitResult {
    pub(crate) balances: HashMap<String, CoinBalance>,
    pub(crate) platform_coin: String,
}
