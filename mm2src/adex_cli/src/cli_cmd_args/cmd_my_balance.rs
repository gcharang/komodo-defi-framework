use clap::Args;
use std::mem::take;

use mm2_rpc::data::legacy::BalanceRequest;

#[derive(Args)]
pub(crate) struct MyBalanceArgs {
    #[arg(name = "COIN", help = "Coin to get balance")]
    coin: String,
}

impl From<&mut MyBalanceArgs> for BalanceRequest {
    fn from(value: &mut MyBalanceArgs) -> Self {
        BalanceRequest {
            coin: take(&mut value.coin),
        }
    }
}
