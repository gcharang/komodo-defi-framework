#[path = "legacy/activation.rs"] mod activation;
#[path = "legacy/orders.rs"] mod orders;
#[path = "legacy/swaps.rs"] mod swaps;
#[path = "legacy/utility.rs"] mod utility;
#[path = "legacy/wallet.rs"] mod wallet;

pub use activation::{eth::GasStationPricePolicy,
                     utxo::{ElectrumProtocol, UtxoMergeParams},
                     CoinInitResponse, EnabledCoin, GetEnabledResponse};
pub use orders::*;
pub use swaps::MySwapsFilter;
pub use utility::{MmVersionResponse, Status, StopRequest, VersionRequest};
pub use wallet::{BalanceRequest, BalanceResponse};

use std::ops::Deref;

use common::serde_derive::{Deserialize, Serialize};

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
