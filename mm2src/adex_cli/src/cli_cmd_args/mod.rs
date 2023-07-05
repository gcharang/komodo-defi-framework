mod cmd_best_orders;
mod cmd_cancel;
mod cmd_my_balance;
mod cmd_order_status;
mod cmd_orderbook;
mod cmd_orderbook_depth;
mod cmd_orders_history;
mod cmd_sell_buy;
mod cmd_set_config;
mod cmd_set_price;
mod cmd_update_maker_order;

use anyhow::Result;
use std::str::FromStr;

use mm2_number::bigdecimal::ParseBigDecimalError;
use mm2_number::{BigDecimal, MmNumber};

pub(crate) mod prelude {
    pub(crate) use super::cmd_best_orders::BestOrderArgs;
    pub(crate) use super::cmd_cancel::CancelSubcommand;
    pub(crate) use super::cmd_my_balance::MyBalanceArgs;
    pub(crate) use super::cmd_order_status::OrderStatusArgs;
    pub(crate) use super::cmd_orderbook::OrderbookArgs;
    pub(crate) use super::cmd_orderbook_depth::OrderbookDepthArgs;
    pub(crate) use super::cmd_orders_history::OrdersHistoryArgs;
    pub(crate) use super::cmd_sell_buy::{BuyOrderArgs, SellOrderArgs};
    pub(crate) use super::cmd_set_config::SetConfigArgs;
    pub(crate) use super::cmd_set_price::SetPriceArgs;
    pub(crate) use super::cmd_update_maker_order::UpdateMakerOrderArgs;
}

fn parse_mm_number(value: &str) -> Result<MmNumber, ParseBigDecimalError> {
    let decimal: BigDecimal = BigDecimal::from_str(value)?;
    Ok(MmNumber::from(decimal))
}
