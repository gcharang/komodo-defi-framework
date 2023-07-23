use clap::Args;
use std::mem::take;

use mm2_rpc::data::legacy::{SetRequiredConfRequest, SetRequiredNotaRequest};

use crate::rpc_data::DisableCoinRequest;

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

#[derive(Args)]
pub(crate) struct SetRequiredConfArgs {
    #[arg(help = "Ticker of the selected coin")]
    pub(crate) coin: String,
    #[arg(visible_alias = "conf", help = "Number of confirmations to require")]
    pub(crate) confirmations: u64,
}

impl From<&mut SetRequiredConfArgs> for SetRequiredConfRequest {
    fn from(value: &mut SetRequiredConfArgs) -> Self {
        SetRequiredConfRequest {
            coin: take(&mut value.coin),
            confirmations: value.confirmations,
        }
    }
}

#[derive(Args)]
pub(crate) struct SetRequiredNotaArgs {
    #[arg(help = "Ticker of the selected coin")]
    pub(crate) coin: String,
    #[arg(
        long,
        short = 'n',
        visible_alias = "requires-nota",
        help = "Whether the node should wait for dPoW notarization of atomic swap transactions"
    )]
    pub(crate) requires_notarization: bool,
}

impl From<&mut SetRequiredNotaArgs> for SetRequiredNotaRequest {
    fn from(value: &mut SetRequiredNotaArgs) -> Self {
        SetRequiredNotaRequest {
            coin: take(&mut value.coin),
            requires_notarization: value.requires_notarization,
        }
    }
}
