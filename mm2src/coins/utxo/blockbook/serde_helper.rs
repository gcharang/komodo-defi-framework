use rpc::v1::types::H256;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::str::FromStr;

pub(super) fn deserialize_hash_string<'de, D>(deserializer: D) -> Result<H256, D::Error>
where
    D: Deserializer<'de>,
{
    let hash: String = Deserialize::deserialize(deserializer)?;
    let hash = H256::from_str(&hash).map_err(Error::custom)?;
    Ok(hash.reversed())
}
