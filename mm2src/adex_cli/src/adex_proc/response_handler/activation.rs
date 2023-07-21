use itertools::Itertools;
use std::io::Write;

use common::write_safe::io::WriteSafeIO;
use common::{write_safe_io, writeln_safe_io};

use super::formatters::ZERO_INDENT;
use crate::rpc_data::{DisableCoinFailed, DisableCoinResponse, DisableCoinSuccess};
use crate::writeln_field;

pub(super) fn on_disable_coin(writer: &mut dyn Write, response: DisableCoinResponse) {
    match response {
        DisableCoinResponse::Success(mm2_rpc_result) => write_disable_success(writer, mm2_rpc_result.result),
        DisableCoinResponse::Failed(disable_failed) => write_disable_failed(writer, disable_failed),
    }
}

macro_rules! write_field_seq {
    ($writer:ident, $seq:expr, $indent:ident) => {
        writeln_field!(
            $writer,
            stringify!($seq),
            if $seq.is_empty() {
                "empty".to_string()
            } else {
                $seq.iter().join(", ")
            },
            $indent
        )
    };
}

fn write_disable_success(writer: &mut dyn Write, disable_success: DisableCoinSuccess) {
    writeln_field!(writer, "coin", disable_success.coin, ZERO_INDENT);
    let cancelled_orders = disable_success.cancelled_orders;
    write_field_seq!(writer, cancelled_orders, ZERO_INDENT);
    writeln_field!(writer, "passivized", disable_success.passivized, ZERO_INDENT);
}

fn write_disable_failed(writer: &mut dyn Write, disable_failed: DisableCoinFailed) {
    writeln_field!(writer, "error", disable_failed.error, ZERO_INDENT);

    let active_swaps = disable_failed.active_swaps;
    write_field_seq!(writer, active_swaps, ZERO_INDENT);
    let orders_matching = disable_failed.orders.matching;
    write_field_seq!(writer, orders_matching, ZERO_INDENT);
    let orders_cancelled = disable_failed.orders.cancelled;
    write_field_seq!(writer, orders_cancelled, ZERO_INDENT);
}
