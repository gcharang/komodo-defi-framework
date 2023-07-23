use itertools::Itertools;
use rpc::v1::types::H256 as H256Json;
use std::io::Write;
use term_table::{row::Row, TableStyle};

use crate::adex_proc::response_handler::formatters::term_table_blank;
use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};

use super::ZERO_INDENT;
use crate::rpc_data::{BanReason, ListBannedPubkeysResponse, UnbanPubkeysResponse};
use crate::{write_field_seq, writeln_field};

pub(super) fn on_list_banned_pubkeys(writer: &mut dyn Write, response: ListBannedPubkeysResponse) {
    if response.is_empty() {
        writeln_field!(writer, "banned_pubkeys", "empty", ZERO_INDENT);
        return;
    }

    let mut term_table = term_table_blank(TableStyle::thin(), false, false, false);
    term_table.add_row(Row::new(["pubkey", "reason", "comment"]));
    for (pubkey, ban_reason) in response {
        match ban_reason {
            BanReason::Manual { reason } => {
                term_table.add_row(Row::new([hex::encode(pubkey.0), "manual".to_string(), reason]))
            },
            BanReason::FailedSwap { caused_by_swap } => term_table.add_row(Row::new([
                hex::encode(pubkey.0),
                "swap".to_string(),
                caused_by_swap.to_string(),
            ])),
        }
    }
    writeln_safe_io!(writer, "{}", term_table.render());
}

pub(super) fn on_unban_pubkeys(writer: &mut dyn Write, response: UnbanPubkeysResponse) {
    let mut still_banned = response.still_banned.iter().map(format_ban_reason);
    write_field_seq!(writer, still_banned, ", ", ZERO_INDENT);
    let mut unbanned = response.unbanned.iter().map(format_ban_reason);
    write_field_seq!(writer, unbanned, ", ", ZERO_INDENT);
    let mut were_not_banned = response.were_not_banned.iter();
    write_field_seq!(writer, were_not_banned, ", ", ZERO_INDENT);
}

fn format_ban_reason((pubkey, ban_reason): (&H256Json, &BanReason)) -> String {
    match ban_reason {
        BanReason::Manual { reason } => {
            format!("{}(manually \"{}\")", hex::encode(pubkey.0), reason)
        },
        BanReason::FailedSwap { caused_by_swap } => {
            format!("{}(caused_by_swap {})", hex::encode(pubkey.0), caused_by_swap)
        },
    }
}
