use derive_more::Display;
use rpc::v1::types::H256 as H256Json;
use std::collections::HashSet;
use std::ops::Deref;
use uuid::Uuid;

use common::serde_derive::{Deserialize, Serialize};
use common::{one_hundred, ten_f64, true_f};
use mm2_number::{construct_detailed, BigDecimal, BigRational, Fraction, MmNumber};

#[derive(Serialize, Deserialize)]
pub struct Mm2RpcResult<T> {
    pub result: T,
}

impl<T> Mm2RpcResult<T> {
    pub fn new(result: T) -> Self { Self { result } }
}

impl<T> Deref for Mm2RpcResult<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &self.result }
}

#[derive(Serialize, Deserialize)]
pub struct BalanceResponse {
    pub coin: String,
    pub balance: BigDecimal,
    pub unspendable_balance: BigDecimal,
    pub address: String,
}

#[derive(Serialize, Deserialize)]
pub struct OrderbookRequest {
    pub base: String,
    pub rel: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderbookResponse {
    #[serde(rename = "askdepth")]
    pub ask_depth: u32,
    pub asks: Vec<AggregatedOrderbookEntry>,
    pub base: String,
    #[serde(rename = "biddepth")]
    pub bid_depth: u32,
    pub bids: Vec<AggregatedOrderbookEntry>,
    pub netid: u16,
    #[serde(rename = "numasks")]
    pub num_asks: usize,
    #[serde(rename = "numbids")]
    pub num_bids: usize,
    pub rel: String,
    pub timestamp: u64,
    #[serde(flatten)]
    pub total_asks_base: TotalAsksBaseVol,
    #[serde(flatten)]
    pub total_asks_rel: TotalAsksRelVol,
    #[serde(flatten)]
    pub total_bids_base: TotalBidsBaseVol,
    #[serde(flatten)]
    pub total_bids_rel: TotalBidsRelVol,
}

construct_detailed!(TotalAsksBaseVol, total_asks_base_vol);
construct_detailed!(TotalAsksRelVol, total_asks_rel_vol);
construct_detailed!(TotalBidsBaseVol, total_bids_base_vol);
construct_detailed!(TotalBidsRelVol, total_bids_rel_vol);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcOrderbookEntry {
    pub coin: String,
    pub address: String,
    pub price: BigDecimal,
    pub price_rat: BigRational,
    pub price_fraction: Fraction,
    #[serde(rename = "maxvolume")]
    pub max_volume: BigDecimal,
    pub max_volume_rat: BigRational,
    pub max_volume_fraction: Fraction,
    pub min_volume: BigDecimal,
    pub min_volume_rat: BigRational,
    pub min_volume_fraction: Fraction,
    pub pubkey: String,
    pub age: u64,
    pub uuid: Uuid,
    pub is_mine: bool,
    #[serde(flatten)]
    pub base_max_volume: DetailedBaseMaxVolume,
    #[serde(flatten)]
    pub base_min_volume: DetailedBaseMinVolume,
    #[serde(flatten)]
    pub rel_max_volume: DetailedRelMaxVolume,
    #[serde(flatten)]
    pub rel_min_volume: DetailedRelMinVolume,
    #[serde(flatten)]
    pub conf_settings: Option<OrderConfirmationsSettings>,
}

construct_detailed!(DetailedBaseMaxVolume, base_max_volume);
construct_detailed!(DetailedBaseMinVolume, base_min_volume);
construct_detailed!(DetailedRelMaxVolume, rel_max_volume);
construct_detailed!(DetailedRelMinVolume, rel_min_volume);

#[derive(Debug, Serialize, Deserialize)]
pub struct AggregatedOrderbookEntry {
    #[serde(flatten)]
    pub entry: RpcOrderbookEntry,
    #[serde(flatten)]
    pub base_max_volume_aggr: AggregatedBaseVol,
    #[serde(flatten)]
    pub rel_max_volume_aggr: AggregatedRelVol,
}

construct_detailed!(AggregatedBaseVol, base_max_volume_aggr);
construct_detailed!(AggregatedRelVol, rel_max_volume_aggr);

#[derive(Deserialize, Serialize, Debug)]
pub struct SellBuyRequest {
    pub base: String,
    pub rel: String,
    pub price: MmNumber,
    pub volume: MmNumber,
    pub timeout: Option<u64>,
    /// Not used. Deprecated.
    #[allow(dead_code)]
    pub duration: Option<u32>,
    pub method: String,
    #[allow(dead_code)]
    pub gui: Option<String>,
    #[serde(rename = "destpubkey")]
    #[serde(default)]
    #[allow(dead_code)]
    pub dest_pub_key: H256Json,
    #[serde(default)]
    pub match_by: MatchBy,
    #[serde(default)]
    pub order_type: OrderType,
    pub base_confs: Option<u64>,
    pub base_nota: Option<bool>,
    pub rel_confs: Option<u64>,
    pub rel_nota: Option<bool>,
    pub min_volume: Option<MmNumber>,
    #[serde(default = "true_f")]
    pub save_in_history: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SellBuyResponse {
    #[serde(flatten)]
    pub request: TakerRequestForRpc,
    pub order_type: OrderType,
    #[serde(flatten)]
    pub min_volume: DetailedMinVolume,
    pub base_orderbook_ticker: Option<String>,
    pub rel_orderbook_ticker: Option<String>,
}

construct_detailed!(DetailedMinVolume, min_volume);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TakerRequestForRpc {
    pub base: String,
    pub rel: String,
    pub base_amount: BigDecimal,
    pub base_amount_rat: BigRational,
    pub rel_amount: BigDecimal,
    pub rel_amount_rat: BigRational,
    pub action: TakerAction,
    pub uuid: Uuid,
    pub method: String,
    pub sender_pubkey: H256Json,
    pub dest_pub_key: H256Json,
    pub match_by: MatchBy,
    pub conf_settings: Option<OrderConfirmationsSettings>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TakerAction {
    Buy,
    Sell,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum OrderType {
    FillOrKill,
    GoodTillCancelled,
}

impl Default for OrderType {
    fn default() -> Self { OrderType::GoodTillCancelled }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MatchBy {
    Any,
    Orders(HashSet<Uuid>),
    Pubkeys(HashSet<H256Json>),
}

impl Default for MatchBy {
    fn default() -> Self { MatchBy::Any }
}

#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrderConfirmationsSettings {
    pub base_confs: u64,
    pub base_nota: bool,
    pub rel_confs: u64,
    pub rel_nota: bool,
}

impl OrderConfirmationsSettings {
    pub fn reversed(&self) -> OrderConfirmationsSettings {
        OrderConfirmationsSettings {
            base_confs: self.rel_confs,
            base_nota: self.rel_nota,
            rel_confs: self.base_confs,
            rel_nota: self.base_nota,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoinInitResponse {
    pub result: String,
    pub address: String,
    pub balance: BigDecimal,
    pub unspendable_balance: BigDecimal,
    pub coin: String,
    pub required_confirmations: u64,
    pub requires_notarization: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mature_confirmations: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct EnabledCoin {
    pub ticker: String,
    pub address: String,
}

pub type GetEnabledResponse = Vec<EnabledCoin>;

#[derive(Serialize, Deserialize, Display)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Success,
}

#[derive(Serialize, Deserialize)]
pub struct MmVersionResponse {
    pub result: String,
    pub datetime: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub enum ActivationRequest {
    Enable(EnableRequest),
    Electrum(ElectrumRequest),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EnableRequest {
    pub coin: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_contract_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_swap_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_station_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_station_decimals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_station_policy: Option<GasStationPricePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mm2: Option<u8>,
    #[serde(default)]
    pub tx_history: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_confirmations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_notarization: Option<bool>,
}

/// Using tagged representation to allow adding variants with coefficients, percentage, etc in the future.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "policy", content = "additional_data")]
pub enum GasStationPricePolicy {
    /// Use mean between average and fast values, default and recommended to use on ETH mainnet due to
    /// gas price big spikes.
    MeanAverageFast,
    /// Use average value only. Useful for non-heavily congested networks (Matic, etc.)
    Average,
}

impl Default for GasStationPricePolicy {
    fn default() -> Self { GasStationPricePolicy::MeanAverageFast }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ElectrumRequest {
    pub coin: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mm2: Option<u8>,
    #[serde(default)]
    pub tx_history: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_confirmations: Option<u64>,
    #[serde(default)]
    pub requires_notarization: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_contract_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_swap_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utxo_merge_params: Option<UtxoMergeParams>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Server {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Protocol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_cert_verification: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Protocol {
    Tcp,
    Ssl,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtxoMergeParams {
    pub merge_at: usize,
    #[serde(default = "ten_f64")]
    pub check_every: f64,
    #[serde(default = "one_hundred")]
    pub max_merge_at_once: usize,
}
