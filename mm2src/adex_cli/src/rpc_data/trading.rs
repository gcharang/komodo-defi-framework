use common::serde_derive::Deserialize;
use mm2_number::Fraction;
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "method", rename = "min_trading_vol")]
pub(crate) struct MinTradingVolRequest {
    pub(crate) coin: String,
}

#[derive(Serialize)]
#[serde(tag = "method", rename = "max_taker_vol")]
pub(crate) struct MaxTakerVolRequest {
    pub(crate) coin: String,
}

#[derive(Deserialize)]
pub(crate) struct MaxTakerVolResponse {
    pub(crate) coin: String,
    pub(crate) result: Fraction,
}
