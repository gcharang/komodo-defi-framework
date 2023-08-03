use anyhow::{anyhow, Result};
use itertools::Itertools;
use std::io::Write;
use term_table::{row::Row, TableStyle};

use common::log::error;
use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io, PagingOptionsEnum};
use mm2_rpc::data::version2::{GetPublicKeyHashResponse, GetPublicKeyResponse, GetRawTransactionResponse};

use super::formatters::{format_bytes, format_datetime, format_datetime_sec, format_ratio, term_table_blank,
                        write_field_option, write_sequence, writeln_field, COMMON_INDENT, COMMON_PRECISION,
                        ZERO_INDENT};
use crate::error_anyhow;
use crate::rpc_data::wallet::{MyTxHistoryDetails, MyTxHistoryResponse, MyTxHistoryResponseV2};
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

pub(super) fn on_tx_history(writer: &mut dyn Write, response: MyTxHistoryResponse) -> Result<()> {
    write_field_option(writer, "from_id", response.from_id.map(format_bytes), ZERO_INDENT);
    writeln_field(writer, "limit", response.limit, ZERO_INDENT);
    writeln_field(writer, "skipped", response.skipped, ZERO_INDENT);
    writeln_field(writer, "total", response.total, ZERO_INDENT);
    write_field_option(writer, "page_number", response.page_number, ZERO_INDENT);
    write_field_option(writer, "total_pages", response.total_pages, ZERO_INDENT);
    writeln_field(writer, "current_block", response.current_block, ZERO_INDENT);
    writeln_field(writer, "sync_status", response.sync_status, ZERO_INDENT);
    write_transactions(writer, response.transactions)?;
    Ok(())
}

pub(super) fn on_tx_history_v2(writer: &mut dyn Write, response: MyTxHistoryResponseV2) -> Result<()> {
    writeln_field(writer, "coin", response.coin, ZERO_INDENT);
    writeln_field(writer, "target", format!("{:?}", response.target), ZERO_INDENT);
    writeln_field(writer, "current_block", response.current_block, ZERO_INDENT);
    writeln_field(writer, "sync_status", response.sync_status, ZERO_INDENT);
    writeln_field(writer, "limit", response.limit, ZERO_INDENT);
    writeln_field(writer, "skipped", response.skipped, ZERO_INDENT);
    writeln_field(writer, "total", response.total, ZERO_INDENT);
    writeln_field(writer, "total_pages", response.total_pages, ZERO_INDENT);
    match response.paging_options {
        PagingOptionsEnum::FromId(bytes) => {
            writeln_field(writer, "from_id", hex::encode(bytes.as_slice()), ZERO_INDENT)
        },
        PagingOptionsEnum::PageNumber(page_number) => writeln_field(writer, "page_number", page_number, ZERO_INDENT),
    }
    write_transactions(writer, response.transactions)
}

fn write_transactions(writer: &mut dyn Write, transactions: Vec<MyTxHistoryDetails>) -> Result<()> {
    if transactions.is_empty() {
        writeln_field(writer, "transactions", "not found", ZERO_INDENT);
    } else {
        writeln_field(writer, "transactions", "", ZERO_INDENT);
        let mut term_table = term_table_blank(TableStyle::thin(), true, false, false);
        term_table.max_column_width = 150;
        for tx in transactions {
            let mut buff: Vec<u8> = vec![];
            let row_writer: &mut dyn Write = &mut buff;
            writeln_field(
                row_writer,
                "time",
                format_datetime_sec(tx.details.timestamp)?,
                ZERO_INDENT,
            );
            writeln_field(row_writer, "coin", tx.details.coin, ZERO_INDENT);
            writeln_field(row_writer, "block", tx.details.block_height, ZERO_INDENT);
            writeln_field(row_writer, "confirmations", tx.confirmations, ZERO_INDENT);
            writeln_field(row_writer, "transaction_type", tx.details.transaction_type, ZERO_INDENT);
            writeln_field(
                row_writer,
                "total_amount",
                format_ratio(&tx.details.total_amount, COMMON_PRECISION)?,
                ZERO_INDENT,
            );
            writeln_field(
                row_writer,
                "spent_by_me",
                format_ratio(&tx.details.spent_by_me, COMMON_PRECISION)?,
                ZERO_INDENT,
            );
            writeln_field(
                row_writer,
                "received_by_me",
                format_ratio(&tx.details.received_by_me, COMMON_PRECISION)?,
                ZERO_INDENT,
            );
            writeln_field(
                row_writer,
                "my_balance_change",
                format_ratio(&tx.details.my_balance_change, COMMON_PRECISION)?,
                ZERO_INDENT,
            );

            write_field_option(row_writer, "fee_details", tx.details.fee_details, ZERO_INDENT);
            if let Some(kmd_rewards) = tx.details.kmd_rewards {
                writeln_field(row_writer, "kmd_rewards", "", ZERO_INDENT);
                writeln_field(
                    row_writer,
                    "amount",
                    format_ratio(&kmd_rewards.amount, COMMON_PRECISION)?,
                    COMMON_INDENT,
                );
                writeln_field(row_writer, "claimed_by_me", kmd_rewards.claimed_by_me, COMMON_INDENT);
            }
            writeln_field(row_writer, "tx_hash", tx.details.tx_hash, ZERO_INDENT);
            writeln_field(row_writer, "from", tx.details.from.iter().join(", "), ZERO_INDENT);
            writeln_field(row_writer, "to", tx.details.to.iter().join(", "), ZERO_INDENT);
            writeln_field(
                row_writer,
                "internal_id",
                format_bytes(tx.details.internal_id),
                ZERO_INDENT,
            );
            write_field_option(row_writer, "memo", tx.details.memo, ZERO_INDENT);
            writeln_field(
                row_writer,
                "tx_hex",
                hex::encode(tx.details.tx_hex.as_slice()),
                ZERO_INDENT,
            );

            let data =
                String::from_utf8(buff).map_err(|error| error_anyhow!("Failed to format tx_history row: {error}"))?;
            term_table.add_row(Row::new([data]))
        }
        writeln_safe_io!(writer, "{}", term_table.render())
    }
    Ok(())
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
