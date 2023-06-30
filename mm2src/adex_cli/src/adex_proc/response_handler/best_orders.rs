use anyhow::Result;
use itertools::Itertools;
use std::io::Write;
use term_table::{row::Row,
                 table_cell::{Alignment, TableCell},
                 TableStyle};

use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};
use mm2_rpc::data::version2::BestOrdersV2Response;

use super::formatters::{format_confirmation_settings, format_ratio, term_table_blank, COMMON_PRECISION};
use super::macros::writeln_field;

pub(super) fn on_best_orders(
    writer: &mut dyn Write,
    response: BestOrdersV2Response,
    show_orig_tickets: bool,
) -> Result<()> {
    if show_orig_tickets {
        writeln_field!(writer, "Original tickers", "", 0);
        for (coin, ticker) in response.original_tickers {
            writeln_field!(writer, coin, ticker.iter().join(","), 8);
        }
        return Ok(());
    }

    let mut term_table = term_table_blank(TableStyle::thin(), false, true, true);
    term_table.add_row(best_orders_table_header_row());

    for (coin, data) in response.orders.iter().sorted_by_key(|p| p.0) {
        term_table.add_row(Row::new(vec![TableCell::new_with_alignment(coin, 7, Alignment::Left)]));
        for order in data.iter().sorted_by_key(|o| o.uuid) {
            term_table.add_row(Row::new(vec![
                TableCell::new(if order.is_mine { "*" } else { "" }),
                TableCell::new(format_ratio(&order.price.rational, COMMON_PRECISION)?),
                TableCell::new(order.uuid),
                TableCell::new(format!(
                    "{}:{}",
                    format_ratio(&order.base_min_volume.rational, COMMON_PRECISION)?,
                    format_ratio(&order.base_max_volume.rational, COMMON_PRECISION)?
                )),
                TableCell::new(format!(
                    "{}:{}",
                    format_ratio(&order.rel_min_volume.rational, COMMON_PRECISION)?,
                    format_ratio(&order.rel_max_volume.rational, COMMON_PRECISION)?
                )),
                TableCell::new(&order.address),
                TableCell::new(
                    &order
                        .conf_settings
                        .as_ref()
                        .map_or_else(|| "none".to_string(), format_confirmation_settings),
                ),
            ]));
        }
    }
    write_safe_io!(writer, "{}", term_table.render());

    Ok(())
}

fn best_orders_table_header_row() -> Row<'static> {
    Row::new(vec![
        TableCell::new(""),
        TableCell::new("Price"),
        TableCell::new("Uuid"),
        TableCell::new("Base vol(min:max)"),
        TableCell::new("Rel vol(min:max)"),
        TableCell::new("Address"),
        TableCell::new("Confirmation"),
    ])
}
