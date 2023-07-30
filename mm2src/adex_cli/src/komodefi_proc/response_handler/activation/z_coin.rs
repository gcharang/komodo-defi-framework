use anyhow::{anyhow, Result};
use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};
use itertools::Itertools;
use std::io::Write;
use term_table::row::Row;
use term_table::TableStyle;

use common::log::error;

use super::super::activation::format_coin_balance;
use super::super::formatters::{term_table_blank, writeln_field, ZERO_INDENT};
use crate::error_anyhow;
use crate::komodefi_proc::response_handler::formatters::COMMON_INDENT;
use crate::rpc_data::activation::{CoinBalanceReport, HDAddressBalance, HDWalletBalance, IguanaWalletBalance,
                                  InitStandaloneCoinResponse, InitStandaloneCoinStatusResponse, RpcDerivationPath,
                                  TaskId, ZcoinActivationResult};

pub(in super::super) fn on_enable_z_coin(
    writer: &mut dyn Write,
    response: InitStandaloneCoinResponse,
) -> Option<TaskId> {
    writeln_field(writer, "task_id", response.task_id, ZERO_INDENT);
    Some(response.task_id)
}

pub(in super::super) fn on_enable_zcoin_status(
    writer: &mut dyn Write,
    response: InitStandaloneCoinStatusResponse,
) -> Result<bool> {
    match response {
        InitStandaloneCoinStatusResponse::Ok(ok_status) => on_enable_zcoin_status_ok(writer, ok_status),
        InitStandaloneCoinStatusResponse::InProgress(_) => {
            write_safe_io!(writer, ".");
            writer.flush()?;
            Ok(true)
        },
        InitStandaloneCoinStatusResponse::UserActionRequired(_) => {
            writeln_field(writer, "status", "user action required", ZERO_INDENT);
            Ok(true)
        },
        InitStandaloneCoinStatusResponse::Error(_) => {
            writeln_field(writer, "status", "error", ZERO_INDENT);
            Ok(false)
        },
    }
}

pub(in super::super) fn on_enable_zcoin_status_ok(
    writer: &mut dyn Write,
    response: ZcoinActivationResult,
) -> Result<bool> {
    writeln_field(writer, "status", "OK", ZERO_INDENT);
    writeln_field(writer, "current_block", response.current_block, ZERO_INDENT);
    writeln_field(writer, "ticker", response.ticker, ZERO_INDENT);
    match response.wallet_balance {
        CoinBalanceReport::Iguana(balance) => write_iguana_balance(writer, balance)?,
        CoinBalanceReport::HD(balance) => write_hd_balance(writer, balance)?,
    }
    Ok(false)
}

fn write_iguana_balance(writer: &mut dyn Write, balance: IguanaWalletBalance) -> Result<()> {
    writeln_field(writer, "iguana wallet", "", ZERO_INDENT);
    writeln_field(writer, "address", balance.address, COMMON_INDENT);
    writeln_field(writer, "balance", format_coin_balance(balance.balance)?, COMMON_INDENT);
    Ok(())
}

fn write_hd_balance(writer: &mut dyn Write, balance: HDWalletBalance) -> Result<()> {
    writeln_field(writer, "hd wallet", "", ZERO_INDENT);
    if balance.accounts.is_empty() {
        writeln_field(writer, "accounts", "none", COMMON_INDENT);
    } else {
        writeln_field(writer, "accounts", "", COMMON_INDENT);
        let mut term_table = term_table_blank(TableStyle::thin(), false, false, false);
        term_table.add_row(Row::new(["index", "derivation", "balance(spend:unspend)", "adresses"]));

        for account in balance.accounts {
            term_table.add_row(Row::new([
                account.account_index.to_string(),
                format_derivation_path(account.derivation_path),
                format_coin_balance(account.total_balance)?,
                format_hd_addresses(account.addresses)?,
            ]))
        }
        writeln_safe_io!(writer, "{}", term_table.render());
    };
    Ok(())
}

fn format_hd_addresses(addresses: Vec<HDAddressBalance>) -> Result<String> {
    let mut buff: Vec<u8> = vec![];
    let writer: &mut dyn Write = &mut buff;
    for address in addresses {
        writeln_field(writer, "address", address.address, ZERO_INDENT);
        writeln_field(writer, "balance", format_coin_balance(address.balance)?, ZERO_INDENT);
        writeln_field(
            writer,
            "der path",
            format_derivation_path(address.derivation_path),
            ZERO_INDENT,
        );
        writeln_field(writer, "chain", address.chain.to_string(), ZERO_INDENT);
        writeln_field(writer, "", "", ZERO_INDENT);
    }
    String::from_utf8(buff).map_err(|error| error_anyhow!("Failed to format hd_address: {error}"))
}

fn format_derivation_path(derivation_path: RpcDerivationPath) -> String {
    derivation_path.0.path.iter().map(|v| v.0).join(",")
}
