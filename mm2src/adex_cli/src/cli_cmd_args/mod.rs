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
use mm2_number::bigdecimal::ParseBigDecimalError;
use mm2_number::{BigDecimal, MmNumber};
use std::str::FromStr;

pub(crate) use cmd_best_orders::*;
pub(crate) use cmd_cancel::*;
pub(crate) use cmd_my_balance::*;
pub(crate) use cmd_order_status::*;
pub(crate) use cmd_orderbook::*;
pub(crate) use cmd_orderbook_depth::*;
pub(crate) use cmd_orders_history::*;
pub(crate) use cmd_sell_buy::*;
pub(crate) use cmd_set_config::*;
pub(crate) use cmd_set_price::*;
pub(crate) use cmd_update_maker_order::*;

pub(crate) fn parse_mm_number(value: &str) -> Result<MmNumber, ParseBigDecimalError> {
    let decimal: BigDecimal = BigDecimal::from_str(value)?;
    Ok(MmNumber::from(decimal))
}
