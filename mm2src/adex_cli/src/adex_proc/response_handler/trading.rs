use anyhow::Result;
use mm2_rpc::data::legacy::MinTradingVolResponse;
use std::io::Write;

use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};
use mm2_number::BigRational;

use super::formatters::{format_ratio, COMMON_PRECISION};
use crate::rpc_data::MaxTakerVolResponse;
use crate::writeln_field;

pub(super) fn on_min_trading_vol(writer: &mut dyn Write, response: MinTradingVolResponse) -> Result<()> {
    writeln_field!(writer, "coin", response.coin, 0);
    writeln_field!(
        writer,
        "volume",
        format_ratio(&response.volume.min_trading_vol, COMMON_PRECISION)?,
        0
    );
    Ok(())
}

pub(super) fn on_max_taker_vol(writer: &mut dyn Write, response: MaxTakerVolResponse) -> Result<()> {
    writeln_field!(writer, "coin", response.coin, 0);
    writeln_field!(
        writer,
        "result",
        format_ratio(&BigRational::from(response.result), COMMON_PRECISION)?,
        0
    );
    Ok(())
}
