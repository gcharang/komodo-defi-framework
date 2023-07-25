use mm2_number::BigDecimal;
use rpc::v1::types::Bytes as BytesJson;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

#[derive(Debug, Serialize)]
#[serde(tag = "method", rename = "send_raw_transaction")]
pub(crate) struct SendRawTransactionRequest {
    pub(crate) coin: String,
    pub(crate) tx_hex: BytesJson,
}

#[derive(Deserialize)]
pub(crate) struct SendRawTransactionResponse {
    pub(crate) tx_hash: BytesJson,
}

#[derive(Debug, Serialize)]
#[serde(tag = "method", rename = "withdraw")]
pub(crate) struct WithdrawRequest {
    pub(crate) coin: String,
    pub(crate) from: Option<WithdrawFrom>,
    pub(crate) to: String,
    #[serde(default)]
    pub(crate) amount: BigDecimal,
    #[serde(default)]
    pub(crate) max: bool,
    pub(crate) fee: Option<WithdrawFee>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum WithdrawFrom {
    AddressId(HDAccountAddressId),
    DerivationPath { derivation_path: String },
}

#[derive(Debug, Serialize)]
pub(crate) struct HDAccountAddressId {
    pub(crate) account_id: u32,
    pub(crate) chain: Bip44Chain,
    pub(crate) address_id: u32,
}

#[derive(Debug, Serialize)]
#[repr(u32)]
pub(crate) enum Bip44Chain {
    External = 0,
    Internal = 1,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(crate) enum WithdrawFee {
    UtxoFixed { amount: BigDecimal },
    UtxoPerKbyte { amount: BigDecimal },
    EthGas { gas_price: BigDecimal, gas: u64 },
    Qrc20Gas { gas_limit: u64, gas_price: u64 },
    CosmosGas { gas_limit: u64, gas_price: f64 },
}

#[derive(Debug, Deserialize)]
pub(crate) struct WithdrawResponse {
    pub(crate) tx_hex: BytesJson,
    pub(crate) tx_hash: String,
    pub(crate) from: Vec<String>,
    pub(crate) to: Vec<String>,
    pub(crate) total_amount: BigDecimal,
    pub(crate) spent_by_me: BigDecimal,
    pub(crate) received_by_me: BigDecimal,
    pub(crate) my_balance_change: BigDecimal,
    pub(crate) block_height: u64,
    pub(crate) timestamp: u64,
    pub(crate) fee_details: Option<Json>,
    pub(crate) coin: String,
    pub(crate) internal_id: BytesJson,
    pub(crate) kmd_rewards: Option<KmdRewardsDetails>,
    pub(crate) transaction_type: Option<Json>,
    pub(crate) memo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KmdRewardsDetails {
    pub(crate) amount: BigDecimal,
    pub(crate) claimed_by_me: bool,
}
