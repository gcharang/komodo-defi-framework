use clap::Args;
use std::mem::take;

use mm2_rpc::data::legacy::MyBalanceRequest;

#[derive(Args)]
pub struct MyBalanceArgs {
    #[arg(name = "COIN", help = "Coin to get balance")]
    coin: String,
}

impl From<&mut MyBalanceArgs> for MyBalanceRequest {
    fn from(value: &mut MyBalanceArgs) -> Self {
        MyBalanceRequest {
            coin: take(&mut value.coin),
        }
    }
}
