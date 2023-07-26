use serde::{Deserialize, Serialize};

use common::true_f;
use mm2_rpc::data::legacy::{ElectrumProtocol, UtxoMergeParams};

#[derive(Deserialize, Serialize)]
pub struct BchWithTokensActivationParams {
    #[serde(flatten)]
    platform_request: BchActivationRequest,
    slp_tokens_requests: Vec<TokenActivationRequest<SlpActivationRequest>>,
    #[serde(default = "true_f")]
    pub get_balances: bool,
}

#[derive(Deserialize, Serialize)]
pub struct BchActivationRequest {
    #[serde(default)]
    allow_slp_unsafe_conf: bool,
    bchd_urls: Vec<String>,
    #[serde(flatten)]
    pub utxo_params: UtxoActivationParams,
}

#[derive(Deserialize, Serialize)]
pub struct UtxoActivationParams {
    pub mode: UtxoRpcMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utxo_merge_params: Option<UtxoMergeParams>,
    #[serde(default)]
    pub tx_history: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_confirmations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_notarization: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_format: Option<UtxoAddressFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_limit: Option<u32>,
    #[serde(flatten)]
    pub enable_params: EnabledCoinBalanceParams,
    #[serde(default)]
    pub priv_key_policy: PrivKeyActivationPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_utxo_maturity: Option<bool>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "rpc", content = "rpc_data")]
pub enum UtxoRpcMode {
    Native,
    Electrum { servers: Vec<ElectrumRpcRequest> },
}

#[derive(Deserialize, Serialize)]
pub struct ElectrumRpcRequest {
    pub url: String,
    #[serde(default)]
    pub protocol: ElectrumProtocol,
    #[serde(default)]
    pub disable_cert_verification: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "format")]
pub enum UtxoAddressFormat {
    /// Standard UTXO address format.
    /// In Bitcoin Cash context the standard format also known as 'legacy'.
    #[serde(rename = "standard")]
    Standard,
    /// Segwit Address
    /// https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki
    #[serde(rename = "segwit")]
    Segwit,
    /// Bitcoin Cash specific address format.
    /// https://github.com/bitcoincashorg/bitcoincash.org/blob/master/spec/cashaddr.md
    #[serde(rename = "cashaddress")]
    CashAddress {
        network: String,
        #[serde(default)]
        pub_addr_prefix: u8,
        #[serde(default)]
        p2sh_addr_prefix: u8,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum PrivKeyActivationPolicy {
    ContextPrivKey,
    Trezor,
}

impl Default for PrivKeyActivationPolicy {
    fn default() -> Self { PrivKeyActivationPolicy::ContextPrivKey }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct EnabledCoinBalanceParams {
    #[serde(default)]
    pub scan_policy: EnableCoinScanPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_addresses_number: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnableCoinScanPolicy {
    DoNotScan,
    ScanIfNewWallet,
    Scan,
}

impl Default for EnableCoinScanPolicy {
    fn default() -> Self { EnableCoinScanPolicy::ScanIfNewWallet }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenActivationRequest<Req> {
    pub(crate) ticker: String,
    #[serde(flatten)]
    pub(crate) request: Req,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlpActivationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_confirmations: Option<u64>,
}
