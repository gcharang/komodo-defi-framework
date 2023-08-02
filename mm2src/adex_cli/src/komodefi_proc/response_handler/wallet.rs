use anyhow::Result;
use std::io::Write;

use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};
use mm2_rpc::data::version2::{GetPublicKeyHashResponse, GetPublicKeyResponse, GetRawTransactionResponse};

use super::formatters::{format_bytes, write_field_option, write_sequence, writeln_field};
use crate::komodefi_proc::response_handler::formatters::{format_datetime, ZERO_INDENT};
use crate::rpc_data::wallet::MyTxHistoryResponse;
use crate::rpc_data::{KmdRewardsDetails, SendRawTransactionResponse, WithdrawResponse};

pub(super) fn on_send_raw_transaction(writer: &mut dyn Write, response: SendRawTransactionResponse, bare_output: bool) {
    let bytes_to_show = hex::encode(response.tx_hash.as_slice());
    if bare_output {
        writeln_safe_io!(writer, "{}", bytes_to_show)
    } else {
        writeln_field(writer, "tx_hash", bytes_to_show, ZERO_INDENT);
    }
}

pub(super) fn on_withdraw(writer: &mut dyn Write, response: WithdrawResponse, bare_output: bool) -> Result<()> {
    if bare_output {
        writeln_safe_io!(writer, "{}", format_bytes(response.tx_hex));
        return Ok(());
    }
    writeln_field(writer, "coin", response.coin, ZERO_INDENT);
    write_sequence(writer, "from", response.from.iter(), ZERO_INDENT);
    write_sequence(writer, "to", response.to.iter(), ZERO_INDENT);
    writeln_field(writer, "total_amount", response.total_amount, ZERO_INDENT);
    writeln_field(writer, "spent_by_me", response.spent_by_me, ZERO_INDENT);
    writeln_field(writer, "received_by_me", response.received_by_me, ZERO_INDENT);
    writeln_field(writer, "my_balance_change", response.my_balance_change, ZERO_INDENT);
    writeln_field(writer, "block_height", response.block_height, ZERO_INDENT);
    writeln_field(writer, "timestamp", format_datetime(response.timestamp)?, ZERO_INDENT);
    write_field_option(writer, "fee_details", response.fee_details, ZERO_INDENT);
    writeln_field(writer, "internal_id", format_bytes(response.internal_id), ZERO_INDENT);
    write_field_option(
        writer,
        "kmd_rewards",
        response.kmd_rewards.map(format_kmd_rewards),
        ZERO_INDENT,
    );
    write_field_option(writer, "transaction_type", response.transaction_type, ZERO_INDENT);
    write_field_option(writer, "memo", response.memo, ZERO_INDENT);

    writeln_field(writer, "tx_hash", response.tx_hash, ZERO_INDENT);
    writeln_field(writer, "tx_hex", format_bytes(response.tx_hex), ZERO_INDENT);

    Ok(())
}

pub(super) fn on_tx_history(writer: &mut dyn Write, response: MyTxHistoryResponse) {
    write_field_option(writer, "from_id", response.from_id.map(format_bytes), ZERO_INDENT);
    writeln_field(writer, "limit", response.limit, ZERO_INDENT);
    writeln_field(writer, "skipped", response.skipped, ZERO_INDENT);
    writeln_field(writer, "total", response.total, ZERO_INDENT);
    write_field_option(writer, "page_number", response.page_number, ZERO_INDENT);
    write_field_option(writer, "total_pages", response.total_pages, ZERO_INDENT);
    writeln_field(writer, "current_block", response.current_block, ZERO_INDENT);
    writeln_field(writer, "sync_status", response.sync_status, ZERO_INDENT);
    if response.transactions.is_empty() {
        writeln_field(writer, "transactions", "not found", ZERO_INDENT);
    } else {
        for tx in response.transactions {
            writeln_safe_io!(writer, "{}", tx)
        }
    }
}

fn format_kmd_rewards(kmd_rewards: KmdRewardsDetails) -> String {
    format!(
        "amount: {}, claimed_by_me: {}",
        kmd_rewards.amount, kmd_rewards.claimed_by_me
    )
}

pub(super) fn on_public_key(writer: &mut dyn Write, response: GetPublicKeyResponse) {
    writeln_field(writer, "public_key", response.public_key, ZERO_INDENT)
}

pub(super) fn on_public_key_hash(writer: &mut dyn Write, response: GetPublicKeyHashResponse) {
    writeln_field(
        writer,
        "public_key_hash",
        hex::encode(response.public_key_hash.0),
        ZERO_INDENT,
    )
}

pub(super) fn on_raw_transaction(writer: &mut dyn Write, response: GetRawTransactionResponse, bare_output: bool) {
    if bare_output {
        writeln_safe_io!(writer, "{}", format_bytes(response.tx_hex))
    } else {
        writeln_field(writer, "tx_hex", format_bytes(response.tx_hex), ZERO_INDENT);
    }
}
