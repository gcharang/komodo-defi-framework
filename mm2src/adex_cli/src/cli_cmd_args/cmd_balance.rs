use clap::Args;
use mm2_rpc::data::legacy::MyBalanceRequest;
use std::mem::take;

#[derive(Args)]
pub struct BalanceArgs {
    #[arg(name = "COIN", help = "Coin to get balance of")]
    coin: String,
}

impl From<&mut BalanceArgs> for MyBalanceRequest {
    fn from(value: &mut BalanceArgs) -> Self {
        MyBalanceRequest {
            coin: take(&mut value.coin),
        }
    }
}
