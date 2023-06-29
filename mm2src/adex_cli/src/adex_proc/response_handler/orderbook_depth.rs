use std::cell::RefMut;
use std::io::Write;
use term_table::{row::Row,
                 table_cell::{Alignment, TableCell},
                 Table as TermTable, TableStyle};

use common::{write_safe::io::WriteSafeIO, write_safe_io};
use mm2_rpc::data::legacy::{Mm2RpcResult, PairWithDepth};

pub(super) fn on_orderbook_depth(
    mut writer: RefMut<'_, dyn Write>,
    mut response: Mm2RpcResult<Vec<PairWithDepth>>,
) -> anyhow::Result<()> {
    let mut term_table = TermTable::with_rows(vec![Row::new(vec![
        TableCell::new(""),
        TableCell::new_with_alignment_and_padding("Bids", 1, Alignment::Left, false),
        TableCell::new_with_alignment_and_padding("Asks", 1, Alignment::Left, false),
    ])]);
    term_table.style = TableStyle::empty();
    term_table.separate_rows = false;
    term_table.has_bottom_boarder = false;
    term_table.has_top_boarder = false;
    response.result.drain(..).for_each(|data| {
        term_table.add_row(Row::new(vec![
            TableCell::new_with_alignment_and_padding(
                format!("{}/{}:", data.pair.0, data.pair.1),
                1,
                Alignment::Right,
                false,
            ),
            TableCell::new_with_alignment_and_padding(data.depth.bids, 1, Alignment::Left, false),
            TableCell::new_with_alignment_and_padding(data.depth.asks, 1, Alignment::Left, false),
        ]))
    });
    write_safe_io!(writer, "{}", term_table.render().replace('\0', ""));
    Ok(())
}
