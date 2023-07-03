use clap::{Args, ValueEnum};
use mm2_number::MmNumber;
use mm2_rpc::data::version2::{BestOrdersAction, BestOrdersRequestV2, RequestBestOrdersBy};
use std::mem::take;

use super::parse_mm_number;

#[derive(Args)]
pub(crate) struct BestOrderArgs {
    #[arg(value_enum, help = "The ticker of the coin to get best orders")]
    pub(crate) coin: String,
    #[arg(help = "Whether to buy or sell the selected coin")]
    pub(crate) action: OrderActionArg,
    #[arg(
        long,
        help = "Whether to show the original tickers if they are configured for the queried coin",
        default_value = "false"
    )]
    pub(crate) show_orig_tickets: bool,
    #[arg(long, help = "Exclude orders that is mine", default_value = "false")]
    pub(crate) exclude_mine: bool,
    #[command(flatten)]
    pub(crate) delegate: BestOrdersByArg,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub(crate) struct BestOrdersByArg {
    #[arg(long, group = "best-orders", value_parser=parse_mm_number, help="The returned results will show the best prices for trades that can fill the requested volume")]
    pub(crate) volume: Option<MmNumber>,
    #[arg(
        long,
        group = "best-orders",
        help = "The returned results will show a list of the best prices"
    )]
    pub(crate) number: Option<usize>,
}

#[derive(Copy, Clone, ValueEnum)]
pub(crate) enum OrderActionArg {
    Buy,
    Sell,
}

impl From<&mut BestOrdersByArg> for RequestBestOrdersBy {
    fn from(value: &mut BestOrdersByArg) -> Self {
        if let Some(number) = value.number {
            RequestBestOrdersBy::Number(number)
        } else if let Some(ref mut volume) = value.volume {
            RequestBestOrdersBy::Volume(take(volume))
        } else {
            panic!("Unreachable state during converting BestOrdersCli into RequestBestOrdersBy");
        }
    }
}

impl From<OrderActionArg> for BestOrdersAction {
    fn from(value: OrderActionArg) -> Self {
        match value {
            OrderActionArg::Buy => BestOrdersAction::Buy,
            OrderActionArg::Sell => BestOrdersAction::Sell,
        }
    }
}

impl From<&mut BestOrderArgs> for BestOrdersRequestV2 {
    fn from(value: &mut BestOrderArgs) -> Self {
        BestOrdersRequestV2 {
            coin: take(&mut value.coin),
            action: value.action.into(),
            request_by: (&mut value.delegate).into(),
            exclude_mine: value.exclude_mine,
        }
    }
}
