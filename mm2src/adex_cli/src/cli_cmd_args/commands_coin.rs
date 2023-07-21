use crate::rpc_data::DisableCoinRequest;
use clap::Args;
use std::mem::take;

#[derive(Args)]
pub(crate) struct DisableCoinArgs {
    #[arg(name = "COIN", help = "Coin to disable")]
    coin: String,
}

impl From<&mut DisableCoinArgs> for DisableCoinRequest {
    fn from(value: &mut DisableCoinArgs) -> Self {
        DisableCoinRequest {
            coin: take(&mut value.coin),
        }
    }
}
