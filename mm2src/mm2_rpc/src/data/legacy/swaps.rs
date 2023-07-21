use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct MySwapsFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<u64>,
}

#[derive(Debug, Deserialize, Display, PartialEq, Serialize)]
pub enum RecoveredSwapAction {
    RefundedMyPayment,
    SpentOtherPayment,
}
