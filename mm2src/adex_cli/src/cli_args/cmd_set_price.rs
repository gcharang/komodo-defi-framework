use clap::Args;
use mm2_number::MmNumber;
use mm2_rpc::data::legacy::SetPriceReq;
use std::mem::take;

use super::parse_mm_number;

#[derive(Args)]
#[command(about = "Places an order on the orderbook, and it relies on this node acting as a maker")]
pub struct SetPriceArgs {
    #[arg(help = "The name of the coin the user desires to sell")]
    base: String,
    #[arg(help = "The name of the coin the user desires to receive")]
    rel: String,
    #[arg(help = "The price in rel the user is willing to receive per one unit of the base coin", value_parser = parse_mm_number)]
    price: MmNumber,
    #[command(flatten)]
    delegate: SetPriceVolumeGroup,
    #[arg(
    long,
    help = "The minimum amount of base coin available for the order; it must be less or equal than volume param; the following values must be greater than or equal to the min_trading_vol of the corresponding coin",
    value_parser = parse_mm_number
    )]
    min_volume: Option<MmNumber>,
    #[arg(long, help = "Cancel all existing orders for the selected pair")]
    cancel_prev: bool,
    #[arg(
        long,
        help = "Number of required blockchain confirmations for base coin atomic swap transaction"
    )]
    base_confs: Option<u64>,
    #[arg(
        long,
        help = "Whether dPoW notarization is required for base coin atomic swap transaction"
    )]
    base_nota: Option<bool>,
    #[arg(
        long,
        help = "Number of required blockchain confirmations for rel coin atomic swap transaction"
    )]
    rel_confs: Option<u64>,
    #[arg(
        long,
        help = "Whether dPoW notarization is required for rel coin atomic swap transaction"
    )]
    rel_nota: Option<bool>,
    #[arg(
        long,
        help = "If true, each order's short record history is stored in a local SQLite database table, and when the order is cancelled or fully matched, it's history will be saved as a json file",
        default_value_t = true
    )]
    save_in_history: bool,
}

impl From<&mut SetPriceArgs> for SetPriceReq {
    fn from(set_price_args: &mut SetPriceArgs) -> Self {
        SetPriceReq {
            base: take(&mut set_price_args.base),
            rel: take(&mut set_price_args.rel),
            price: take(&mut set_price_args.price),
            max: set_price_args.delegate.max,
            volume: set_price_args
                .delegate
                .volume
                .as_mut()
                .map_or_else(MmNumber::default, take),
            min_volume: take(&mut set_price_args.min_volume),
            cancel_previous: set_price_args.cancel_prev,
            base_confs: take(&mut set_price_args.base_confs),
            base_nota: take(&mut set_price_args.base_nota),
            rel_confs: take(&mut set_price_args.rel_confs),
            rel_nota: take(&mut set_price_args.rel_nota),
            save_in_history: set_price_args.save_in_history,
        }
    }
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct SetPriceVolumeGroup {
    #[arg(
        group = "set-price-volume",
        long,
        help = "Use the entire coin balance for the order, taking 0.001 coins into reserve to account for fees",
        default_value_t = false
    )]
    max: bool,
    #[arg(
        group = "set-price-volume",
        long,
        help = "The maximum amount of base coin available for the order, ignored if max is true; the following values must be greater than or equal to the min_trading_vol of the corresponding coin",
        value_parser = parse_mm_number
    )]
    volume: Option<MmNumber>,
}
