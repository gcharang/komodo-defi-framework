use super::*;
use mm2_rpc::data::version2::{BestOrdersAction, BestOrdersRequestV2, RequestBestOrdersBy};

#[derive(Args)]
pub struct BestOrderArgs {
    #[arg(help = "Whether to buy or sell the selected coin")]
    pub coin: String,
    #[arg(value_enum, help = "The ticker of the coin to get best orders")]
    pub action: OrderActionArg,
    #[arg(
        long,
        help = "Tickers included in response when orderbook_ticker is configured for the queried coin in coins file",
        default_value = "false"
    )]
    pub show_orig_tickets: bool,
    #[arg(long, help = "Excludes orders that is mine", default_value = "false")]
    pub exclude_mine: bool,
    #[command(flatten)]
    pub delegate: BestOrdersByArg,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct BestOrdersByArg {
    #[arg(long, group = "best-orders", value_parser=parse_mm_number, help="The returned results will show the best prices for trades that can fill the requested volume")]
    pub volume: Option<MmNumber>,
    #[arg(
        long,
        group = "best-orders",
        help = "The returned results will show a list of the best prices"
    )]
    pub number: Option<usize>,
}

#[derive(Copy, Clone, ValueEnum)]
pub enum OrderActionArg {
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
            exclude_my: Some(value.exclude_mine),
        }
    }
}
