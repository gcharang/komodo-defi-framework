use clap::Args;
use std::mem::take;

use mm2_rpc::data::legacy::MyBalanceRequest;

#[derive(Args)]
pub struct BalanceArgs {
    #[arg(name = "COIN", help = "Coin to get balance")]
    coin: String,
}

impl From<&mut BalanceArgs> for MyBalanceRequest {
    fn from(value: &mut BalanceArgs) -> Self {
        MyBalanceRequest {
            coin: take(&mut value.coin),
        }
    }
}
