use common::serde_derive::{Deserialize, Serialize};
use common::{one_hundred, ten_f64};
use mm2_number::BigDecimal;

pub use electrum::{ElectrumProtocol, UtxoMergeParams};
pub use enable::GasStationPricePolicy;

#[derive(Serialize, Deserialize)]
pub struct EnabledCoin {
    pub ticker: String,
    pub address: String,
}

pub type GetEnabledResponse = Vec<EnabledCoin>;

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

mod electrum {
    use super::*;

    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct UtxoMergeParams {
        pub merge_at: usize,
        #[serde(default = "ten_f64")]
        pub check_every: f64,
        #[serde(default = "one_hundred")]
        pub max_merge_at_once: usize,
    }

    #[allow(clippy::upper_case_acronyms)]
    #[derive(Clone, Debug, Deserialize, Serialize)]
    /// Deserializable Electrum protocol representation for RPC
    pub enum ElectrumProtocol {
        /// TCP
        TCP,
        /// SSL/TLS
        SSL,
        /// Insecure WebSocket.
        WS,
        /// Secure WebSocket.
        WSS,
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl Default for ElectrumProtocol {
        fn default() -> Self { ElectrumProtocol::TCP }
    }

    #[cfg(target_arch = "wasm32")]
    impl Default for ElectrumProtocol {
        fn default() -> Self { ElectrumProtocol::WS }
    }
}

mod enable {
    use super::*;

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
}
