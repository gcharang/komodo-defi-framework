use rpc::v1::types::H256;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::str::FromStr;

mod amount {
    use bitcoin::Amount;
    use bitcoin::Denomination::Satoshi;

    pub(crate) struct AmountVisitor;

    impl<'de> serde::de::Visitor<'de> for AmountVisitor {
        type Value = Amount;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a valid Bitcoin amount")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if let Ok(amount) = Amount::from_btc(value) {
                Ok(amount)
            } else {
                Err(E::custom("invalid Bitcoin amount"))
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if let Ok(amount) = Amount::from_str_in(value, Satoshi) {
                Ok(amount)
            } else {
                Err(E::custom("invalid Bitcoin amount"))
            }
        }
    }
}

pub(super) fn deserialize_amount<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let amount = deserializer.deserialize_any(amount::AmountVisitor)?;
    Ok(Some(amount.to_btc()))
}

pub(super) fn deserialize_hex_string<'de, D>(deserializer: D) -> Result<H256, D::Error>
where
    D: Deserializer<'de>,
{
    let hash: String = Deserialize::deserialize(deserializer)?;
    let hash = H256::from_str(&hash).map_err(Error::custom)?;
    Ok(hash.reversed())
}
