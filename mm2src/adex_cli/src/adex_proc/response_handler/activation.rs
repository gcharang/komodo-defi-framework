use itertools::Itertools;
use std::io::Write;

use common::write_safe::io::WriteSafeIO;
use common::{write_safe_io, writeln_safe_io};

use super::formatters::ZERO_INDENT;
use crate::rpc_data::{CoinsToKickstartResponse, DisableCoinFailed, DisableCoinResponse, DisableCoinSuccess,
                      SetRequiredConfResponse, SetRequiredNotaResponse};
use crate::{write_field_seq, writeln_field};

pub(super) fn on_disable_coin(writer: &mut dyn Write, response: DisableCoinResponse) {
    match response {
        DisableCoinResponse::Success(mm2_rpc_result) => write_disable_success(writer, mm2_rpc_result.result),
        DisableCoinResponse::Failed(disable_failed) => write_disable_failed(writer, disable_failed),
    }
}

pub(super) fn on_set_confirmations(writer: &mut dyn Write, response: SetRequiredConfResponse) {
    writeln_field!(writer, "coin", response.coin, ZERO_INDENT);
    writeln_field!(writer, "confirmations", response.confirmations, ZERO_INDENT);
}

pub(super) fn on_set_notarization(writer: &mut dyn Write, response: SetRequiredNotaResponse) {
    writeln_field!(writer, "coin", response.coin, ZERO_INDENT);
    writeln_field!(
        writer,
        "requires_notarization",
        response.requires_notarization,
        ZERO_INDENT
    );
}

pub(super) fn on_coins_to_kickstart(writer: &mut dyn Write, coins: CoinsToKickstartResponse) {
    write_field_seq!(writer, coins, ", ", ZERO_INDENT);
}

fn write_disable_success(writer: &mut dyn Write, disable_success: DisableCoinSuccess) {
    writeln_field!(writer, "coin", disable_success.coin, ZERO_INDENT);
    let mut cancelled_orders = disable_success.cancelled_orders.iter();
    write_field_seq!(writer, cancelled_orders, ", ", ZERO_INDENT);
    writeln_field!(writer, "passivized", disable_success.passivized, ZERO_INDENT);
}

fn write_disable_failed(writer: &mut dyn Write, disable_failed: DisableCoinFailed) {
    writeln_field!(writer, "error", disable_failed.error, ZERO_INDENT);

    let mut active_swaps = disable_failed.active_swaps.iter();
    write_field_seq!(writer, active_swaps, ", ", ZERO_INDENT);
    let mut orders_matching = disable_failed.orders.matching.iter();
    write_field_seq!(writer, orders_matching, ", ", ZERO_INDENT);
    let mut orders_cancelled = disable_failed.orders.cancelled.iter();
    write_field_seq!(writer, orders_cancelled, ", ", ZERO_INDENT);
}
