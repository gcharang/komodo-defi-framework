mod cmd_best_orders;
mod cmd_buy_sell;
mod cmd_orderbook;
mod cmd_orderbook_depth;
mod cmd_set_price;

use anyhow::{anyhow, Result};
use clap::{Args, ValueEnum};
use mm2_number::bigdecimal::ParseBigDecimalError;
use mm2_number::{BigDecimal, MmNumber};
use std::mem::take;
use std::str::FromStr;

pub use cmd_best_orders::*;
pub use cmd_buy_sell::*;
pub use cmd_orderbook::*;
pub use cmd_orderbook_depth::*;
pub use cmd_set_price::*;

pub(crate) fn parse_mm_number(value: &str) -> Result<MmNumber, ParseBigDecimalError> {
    let decimal: BigDecimal = BigDecimal::from_str(value)?;
    Ok(MmNumber::from(decimal))
}
