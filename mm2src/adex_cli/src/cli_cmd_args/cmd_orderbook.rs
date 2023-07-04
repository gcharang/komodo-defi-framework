use clap::Args;
use common::serde_derive::Serialize;
use mm2_rpc::data::legacy::OrderbookRequest;

use crate::adex_proc::OrderbookSettings;

const ORDERBOOK_BIDS_LIMIT: &str = "20";
const ORDERBOOK_ASKS_LIMIT: &str = "20";

#[derive(Args, Serialize, Debug)]
pub(crate) struct OrderbookArgs {
    #[arg(help = "Base currency of a pair")]
    base: String,
    #[arg(help = "Related currency, also can be called \"quote currency\" according to exchange terms")]
    rel: String,
    #[arg(long, help = "Orderbook asks count limitation", default_value = ORDERBOOK_ASKS_LIMIT)]
    asks_limit: Option<usize>,
    #[arg(long, help = "Orderbook bids count limitation", default_value = ORDERBOOK_BIDS_LIMIT)]
    bids_limit: Option<usize>,
    #[arg(long, short, help = "Enables `uuid` column")]
    uuids: bool,
    #[arg(long, visible_alias = "min", help = "Enables `min_volume` column")]
    min_volume: bool,
    #[arg(long, visible_alias = "max", help = "Enables `max_volume` column")]
    max_volume: bool,
    #[arg(long, short, help = "Enables `public` column")]
    publics: bool,
    #[arg(long, short, help = "Enables `address` column")]
    address: bool,
    #[arg(long, help = "Enables `age` column")]
    age: bool,
    #[arg(long, visible_alias = "cs", help = "Enables order confirmation settings column")]
    conf_settings: bool,
}

impl From<&OrderbookArgs> for OrderbookSettings {
    fn from(value: &OrderbookArgs) -> Self {
        OrderbookSettings {
            uuids: value.uuids,
            min_volume: value.min_volume,
            max_volume: value.max_volume,
            publics: value.publics,
            address: value.address,
            age: value.age,
            conf_settings: value.conf_settings,
            asks_limit: value.asks_limit,
            bids_limit: value.bids_limit,
        }
    }
}

impl From<&OrderbookArgs> for OrderbookRequest {
    fn from(value: &OrderbookArgs) -> Self {
        OrderbookRequest {
            rel: value.rel.clone(),
            base: value.base.clone(),
        }
    }
}
