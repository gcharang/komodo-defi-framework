mod formatters;
mod orderbook;

use chrono::{TimeZone, Utc};
use common::io::{write_safe_io, writeln_safe_io, WriteSafeIO};
use itertools::Itertools;
use log::{error, info};
use mm2_number::bigdecimal::{FromPrimitive, ToPrimitive};
use mm2_number::{BigDecimal, BigRational};
use mm2_rpc_data::legacy::{BalanceResponse, CancelAllOrdersResponse, CoinInitResponse, GetEnabledResponse,
                           HistoricalOrder, MakerMatchForRpc, MakerOrderForMyOrdersRpc, MakerOrderForRpc,
                           MakerReservedForRpc, MatchBy, Mm2RpcResult, MmVersionResponse, MyOrdersResponse,
                           OrderConfirmationsSettings, OrderStatusResponse, OrderbookResponse, SellBuyResponse,
                           Status, TakerAction, TakerMatchForRpc, TakerOrderForRpc, TakerRequestForRpc};
use mm2_rpc_data::version2::BestOrdersV2Response;
use rpc::v1::types::H256 as H256Json;
use serde_json::ser::CharEscape::Tab;
use serde_json::Value as Json;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::io::Write;
use std::ops::DerefMut;
use std::str::FromStr;
use std::string::ToString;
use uuid::Uuid;

use term_table::row::Row;
use term_table::table_cell::TableCell;
use term_table::{Table as TermTable, TableStyle};

use super::OrderbookConfig;
use crate::adex_config::AdexConfig;
use crate::adex_proc::response_handler::formatters::smart_fraction_fmt::SmartFractionFmt;

const COMMON_INDENT: usize = 20;
const NESTED_INDENT: usize = 26;

pub(crate) trait ResponseHandler {
    fn print_response(&self, response: Json) -> Result<(), ()>;
    fn debug_response<T: Debug + 'static>(&self, response: &T) -> Result<(), ()>;
    fn on_orderbook_response<Cfg: AdexConfig + 'static>(
        &self,
        orderbook: OrderbookResponse,
        config: &Cfg,
        otderbook_config: OrderbookConfig,
    ) -> Result<(), ()>;
    fn on_get_enabled_response(&self, enabled: &Mm2RpcResult<GetEnabledResponse>) -> Result<(), ()>;
    fn on_version_response(&self, response: &MmVersionResponse) -> Result<(), ()>;
    fn on_enable_response(&self, response: &CoinInitResponse) -> Result<(), ()>;
    fn on_balance_response(&self, response: &BalanceResponse) -> Result<(), ()>;
    fn on_sell_response(&self, response: &Mm2RpcResult<SellBuyResponse>) -> Result<(), ()>;
    fn on_buy_response(&self, response: &Mm2RpcResult<SellBuyResponse>) -> Result<(), ()>;
    fn on_stop_response(&self, response: &Mm2RpcResult<Status>) -> Result<(), ()>;
    fn on_cancel_order_response(&self, response: &Mm2RpcResult<Status>) -> Result<(), ()>;
    fn on_cancel_all_response(&self, response: &Mm2RpcResult<CancelAllOrdersResponse>) -> Result<(), ()>;
    fn on_order_status(&self, response: &OrderStatusResponse) -> Result<(), ()>;
    fn on_best_orders(&self, best_orders: BestOrdersV2Response, show_orig_tickets: bool) -> Result<(), ()>;
    fn on_my_orders(&self, my_orders: MyOrdersResponse) -> Result<(), ()>;
}

pub(crate) struct ResponseHandlerImpl<'a> {
    pub writer: RefCell<&'a mut dyn Write>,
}

impl<'a> ResponseHandler for ResponseHandlerImpl<'a> {
    fn print_response(&self, result: Json) -> Result<(), ()> {
        let mut binding = self.writer.borrow_mut();
        let writer = binding.deref_mut();

        let object = result
            .as_object()
            .ok_or_else(|| error!("Failed to cast result as object"))?;

        object
            .iter()
            .for_each(|value| writeln_safe_io!(writer, "{}: {:?}", value.0, value.1));
        Ok(())
    }

    fn debug_response<T: Debug + 'static>(&self, response: &T) -> Result<(), ()> {
        info!("{response:?}");
        Ok(())
    }

    fn on_orderbook_response<Cfg: AdexConfig + 'static>(
        &self,
        orderbook: OrderbookResponse,
        config: &Cfg,
        otderbook_config: OrderbookConfig,
    ) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();

        let base_vol_head = "Volume: ".to_string() + &orderbook.base;
        let rel_price_head = "Price: ".to_string() + &orderbook.rel;
        writeln_safe_io!(
            writer,
            "{}",
            orderbook::AskBidRow::new(
                base_vol_head.as_str(),
                rel_price_head.as_str(),
                "Uuid",
                "Min volume",
                "Max volume",
                "Age(sec.)",
                "Public",
                "Address",
                "Order conf (bc,bn:rc,rn)",
                &otderbook_config
            )
        );

        let price_prec = config.orderbook_price_precision();
        let vol_prec = config.orderbook_volume_precision();

        if orderbook.asks.is_empty() {
            writeln_safe_io!(
                writer,
                "{}",
                orderbook::AskBidRow::new("", "No asks found", "", "", "", "", "", "", "", &otderbook_config)
            );
        } else {
            let skip = orderbook
                .asks
                .len()
                .checked_sub(otderbook_config.asks_limit.unwrap_or(usize::MAX))
                .unwrap_or_default();

            orderbook
                .asks
                .iter()
                .sorted_by(orderbook::cmp_asks)
                .skip(skip)
                .map(|entry| orderbook::AskBidRow::from_orderbook_entry(entry, vol_prec, price_prec, &otderbook_config))
                .for_each(|row: orderbook::AskBidRow| writeln_safe_io!(writer, "{}", row));
        }
        writeln_safe_io!(writer, "{}", orderbook::AskBidRow::new_delimiter(&otderbook_config));

        if orderbook.bids.is_empty() {
            writeln_safe_io!(
                writer,
                "{}",
                orderbook::AskBidRow::new("", "No bids found", "", "", "", "", "", "", "", &otderbook_config)
            );
        } else {
            orderbook
                .bids
                .iter()
                .sorted_by(orderbook::cmp_bids)
                .take(otderbook_config.bids_limit.unwrap_or(usize::MAX))
                .map(|entry| orderbook::AskBidRow::from_orderbook_entry(entry, vol_prec, price_prec, &otderbook_config))
                .for_each(|row: orderbook::AskBidRow| writeln_safe_io!(writer, "{}", row));
        }
        Ok(())
    }

    fn on_get_enabled_response(&self, enabled: &Mm2RpcResult<GetEnabledResponse>) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();
        writeln_safe_io!(writer, "{:8} {}", "Ticker", "Address");
        for row in &enabled.result {
            writeln_safe_io!(writer, "{:8} {}", row.ticker, row.address);
        }
        Ok(())
    }

    fn on_version_response(&self, response: &MmVersionResponse) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();
        writeln_safe_io!(writer, "Version: {}", response.result);
        writeln_safe_io!(writer, "Datetime: {}", response.datetime);
        Ok(())
    }

    fn on_enable_response(&self, response: &CoinInitResponse) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();
        writeln_safe_io!(
            writer,
            "coin: {}\naddress: {}\nbalance: {}\nunspendable_balance: {}\nrequired_confirmations: {}\nrequires_notarization: {}",
            response.coin,
            response.address,
            response.balance,
            response.unspendable_balance,
            response.required_confirmations,
            if response.requires_notarization { "Yes" } else { "No" }
        );
        if let Some(mature_confirmations) = response.mature_confirmations {
            writeln_safe_io!(writer, "mature_confirmations: {}", mature_confirmations);
        }
        Ok(())
    }

    fn on_balance_response(&self, response: &BalanceResponse) -> Result<(), ()> {
        writeln_safe_io!(
            self.writer.borrow_mut(),
            "coin: {}\nbalance: {}\nunspendable: {}\naddress: {}",
            response.coin,
            response.balance,
            response.unspendable_balance,
            response.address
        );
        Ok(())
    }

    fn on_sell_response(&self, response: &Mm2RpcResult<SellBuyResponse>) -> Result<(), ()> {
        writeln_safe_io!(self.writer.borrow_mut(), "Order uuid: {}", response.request.uuid);
        Ok(())
    }

    fn on_buy_response(&self, response: &Mm2RpcResult<SellBuyResponse>) -> Result<(), ()> {
        writeln_safe_io!(self.writer.borrow_mut(), "{}", response.request.uuid);
        Ok(())
    }

    fn on_stop_response(&self, response: &Mm2RpcResult<Status>) -> Result<(), ()> {
        match response.result {
            Status::Success => writeln_safe_io!(self.writer.borrow_mut(), "Service stopped"),
        }
        Ok(())
    }

    fn on_cancel_order_response(&self, response: &Mm2RpcResult<Status>) -> Result<(), ()> {
        match response.result {
            Status::Success => writeln_safe_io!(self.writer.borrow_mut(), "Order cancelled"),
        }
        Ok(())
    }

    fn on_cancel_all_response(&self, response: &Mm2RpcResult<CancelAllOrdersResponse>) -> Result<(), ()> {
        let cancelled = &response.result.cancelled;
        let mut writer = self.writer.borrow_mut();
        if cancelled.is_empty() {
            writeln_safe_io!(writer, "No orders found to be cancelled");
        } else {
            writeln_safe_io!(writer, "Cancelled: {}", cancelled.iter().join(", "));
        }

        let currently_matched = &response.result.currently_matching;
        if !currently_matched.is_empty() {
            writeln_safe_io!(writer, "Currently matched: {}", currently_matched.iter().join(", "));
        }
        Ok(())
    }

    fn on_order_status(&self, response: &OrderStatusResponse) -> Result<(), ()> {
        let mut binding = self.writer.borrow_mut();
        let mut writer: &mut dyn Write = binding.deref_mut();
        match response {
            OrderStatusResponse::Maker(maker_status) => self.print_maker_order(writer, maker_status)?,
            OrderStatusResponse::Taker(taker_status) => self.print_taker_order(&mut writer, taker_status)?,
        }
        Ok(())
    }

    fn on_best_orders(&self, best_orders: BestOrdersV2Response, show_orig_tickets: bool) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();
        if show_orig_tickets {
            writeln_field!(writer, "Original tickers", "", 0);
            for (coin, ticker) in best_orders.original_tickers {
                writeln_field!(writer, coin, ticker.iter().join(","), 8);
            }
            return Ok(());
        }

        macro_rules! fract {
            ($field:expr) => {
                SmartFractionFmt::new(2, 5, $field.rational.to_f64().unwrap())
                    .unwrap()
                    .to_string()
            };
        }

        #[derive(Table)]
        struct BestOrdersRow {
            #[table(title = "")]
            is_mine: &'static str,
            #[table(title = "Price")]
            price: String,
            #[table(title = "Base Vol.")]
            base_vol: String,
            #[table(title = "Rel Vol.")]
            rel_vol: String,
            #[table(title = "Uuid")]
            uuid: Uuid,
            #[table(title = "Address")]
            address: String,
            #[table(title = "Confirmation")]
            conf_settings: String,
        }
        for (coin, mut data) in best_orders.orders {
            writeln_safe_io!(writer, "{}: ", coin);
            let rows: Vec<BestOrdersRow> = data
                .iter_mut()
                .map(|value| BestOrdersRow {
                    is_mine: if value.is_mine { "*" } else { "" },
                    price: fract!(value.price),
                    uuid: value.uuid,
                    address: value.address.to_string(),
                    base_vol: format!("{}:{}", fract!(value.base_min_volume), fract!(value.base_max_volume)),
                    rel_vol: format!("{}:{}", fract!(value.rel_min_volume), fract!(value.rel_max_volume)),
                    conf_settings: value
                        .conf_settings
                        .map_or("".to_string(), |conf| format_confirmation_settings(&conf)),
                })
                .collect();

            let table = rows
                .with_title()
                .separator(Separator::builder().build())
                .border(Border::builder().build())
                .display()
                .unwrap();
            writeln_safe_io!(writer, "{}", table);
        }

        Ok(())
    }

    fn on_my_orders(&self, my_orders: MyOrdersResponse) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();
        let writer: &mut dyn Write = writer.deref_mut();
        writeln_safe_io!(writer, "{}", Self::format_taker_orders_table(&my_orders.taker_orders)?);
        writeln_safe_io!(writer, "{}", Self::format_maker_orders_table(&my_orders.maker_orders)?);
        Ok(())
    }
}

impl ResponseHandlerImpl<'_> {
    fn print_maker_order(&self, writer: &mut dyn Write, maker_status: &MakerOrderForMyOrdersRpc) -> Result<(), ()> {
        let order = &maker_status.order;
        writeln_field!(writer, "base", order.base, COMMON_INDENT);
        writeln_field!(writer, "rel", order.rel, COMMON_INDENT);
        writeln_field!(writer, "price", format_ratio(&order.price_rat, 2, 5)?, COMMON_INDENT);
        writeln_field!(writer, "uuid", order.uuid, COMMON_INDENT);
        writeln_field!(writer, "created at", format_datetime(order.created_at)?, COMMON_INDENT);
        if let Some(updated_at) = order.updated_at {
            writeln_field!(writer, "updated at", format_datetime(updated_at)?, COMMON_INDENT);
        }
        writeln_field!(
            writer,
            "max_base_vol",
            format_ratio(&order.max_base_vol_rat, 2, 5)?,
            COMMON_INDENT
        );
        writeln_field!(
            writer,
            "min_base_vol",
            format_ratio(&order.min_base_vol_rat, 2, 5)?,
            COMMON_INDENT
        );
        writeln_field!(
            writer,
            "swaps",
            if order.started_swaps.is_empty() {
                "empty".to_string()
            } else {
                format!("{}", order.started_swaps.iter().join(", "))
            },
            COMMON_INDENT
        );

        if let Some(ref conf_settings) = order.conf_settings {
            writeln_field!(
                writer,
                "conf_settings",
                format_confirmation_settings(conf_settings),
                COMMON_INDENT
            );
        }
        if let Some(ref changes_history) = order.changes_history {
            writeln_field!(
                writer,
                "changes_history",
                changes_history
                    .iter()
                    .map(|val| format_historical_changes(val, ", ").unwrap_or_else(|_| "error".into()))
                    .join(", "),
                COMMON_INDENT
            );
        }

        writeln_field!(writer, "cancellable", maker_status.cancellable, COMMON_INDENT);
        writeln_field!(
            writer,
            "available_amount",
            format_ratio(&maker_status.available_amount, 2, 5)?,
            COMMON_INDENT
        );

        Self::write_maker_matches(writer, &order.matches)?;
        writeln_safe_io!(writer, "");
        Ok(())
    }

    fn write_maker_matches(writer: &mut dyn Write, matches: &HashMap<Uuid, MakerMatchForRpc>) -> Result<(), ()> {
        if matches.is_empty() {
            //    write_field!(writer, "matches", "empty", COMMON_INDENT);
            return Ok(());
        }
        //write_field!(writer, "matches", "", COMMON_INDENT);
        for (uuid, m) in matches {
            Self::write_maker_match(writer, uuid, m)?
        }
        Ok(())
    }

    fn write_maker_match(writer: &mut dyn Write, uuid: &Uuid, m: &MakerMatchForRpc) -> Result<(), ()> {
        let (req, reserved, connect, connected) = (&m.request, &m.reserved, &m.connect, &m.connected);
        writeln_field!(writer, "uuid", uuid, NESTED_INDENT);
        writeln_field!(writer, "req.uuid", req.uuid, NESTED_INDENT);
        write_base_rel!(writer, req, NESTED_INDENT);
        writeln_field!(
            writer,
            "req.match_by",
            format_match_by(&req.match_by, ", "),
            NESTED_INDENT
        );
        writeln_field!(writer, "req.action", req.action, NESTED_INDENT);
        write_confirmation_settings!(writer, req, NESTED_INDENT);
        writeln_field!(
            writer,
            "req.(sender, dest)",
            format!("{},{}", req.sender_pubkey, req.dest_pub_key),
            NESTED_INDENT
        );
        Self::write_maker_reserved_for_rpc(writer, reserved);

        if let Some(ref connected) = connected {
            write_connected!(writer, connected, NESTED_INDENT);
        }

        if let Some(ref connect) = connect {
            write_connected!(writer, connect, NESTED_INDENT);
        }

        write_field!(writer, "last_updated", format_datetime(m.last_updated)?, NESTED_INDENT);
        Ok(())
    }

    fn taker_order_header_row() -> Row<'static> {
        Row::new(vec![
            TableCell::new("action\nbase(vol),rel(vol)"),
            TableCell::new("uuid, sender, dest"),
            TableCell::new("type,created_at\nconfirmation"),
            TableCell::new("match_by"),
            TableCell::new("base,rel\norderbook ticker"),
            TableCell::new("cancellable"),
        ])
    }

    fn taker_order_row(taker_order: &TakerOrderForRpc) -> Result<Vec<Row>, ()> {
        let req = &taker_order.request;
        let mut rows = vec![Row::new(vec![
            TableCell::new(format!(
                "{}\n{}({}),{}({})",
                req.action,
                req.base,
                format_ratio(&req.base_amount, 2, 5)?,
                req.rel,
                format_ratio(&req.rel_amount, 2, 5)?
            )),
            TableCell::new(format!("{}\n{}\n{}", req.uuid, req.sender_pubkey, req.dest_pub_key)),
            TableCell::new(format!(
                "{}\n{}\n{}",
                taker_order.order_type,
                format_datetime(taker_order.created_at)?,
                req.conf_settings
                    .as_ref()
                    .map_or_else(|| "none".to_string(), |val| format_confirmation_settings(val)),
            )),
            TableCell::new(format_match_by(&req.match_by, "\n")),
            TableCell::new(format!(
                "{}\n{}",
                taker_order
                    .base_orderbook_ticker
                    .as_ref()
                    .map_or_else(|| "none".to_string(), String::clone),
                taker_order
                    .rel_orderbook_ticker
                    .as_ref()
                    .map_or_else(|| "none".to_string(), String::clone)
            )),
            TableCell::new(taker_order.cancellable),
        ])];

        if taker_order.matches.is_empty() {
            return Ok(rows);
        }
        rows.push(Row::new(vec![TableCell::new_with_col_span("matches", 6)]));
        for (uuid, m) in taker_order.matches.iter() {
            let mut matches_str = Vec::new();
            let mut buf: Box<dyn Write> = Box::new(&mut matches_str);
            Self::write_taker_match(buf.as_mut(), uuid, m)?;
            drop(buf);
            rows.push(Row::new(vec![TableCell::new_with_col_span(
                String::from_utf8(matches_str).unwrap(),
                6,
            )]));
        }

        Ok(rows)
    }

    fn format_maker_orders_table(maker_orders: &HashMap<Uuid, MakerOrderForMyOrdersRpc>) -> Result<String, ()> {
        let mut buff = Vec::new();
        let mut writer: Box<dyn Write> = Box::new(&mut buff);

        if maker_orders.is_empty() {
            writeln_field!(writer, "Maker orders", "empty", COMMON_INDENT);
        } else {
            writeln_field!(writer, "Maker orders", "", COMMON_INDENT);
            let mut table = TermTable::new();
            table.style = TableStyle::thin();
            table.add_row(ResponseHandlerImpl::maker_order_header_row());

            for (_, maker_order) in maker_orders.iter().sorted_by_key(|(uuid, _)| *uuid) {
                for row in ResponseHandlerImpl::maker_order_row(maker_order)? {
                    table.add_row(row);
                }
            }
            write_safe_io!(writer, "{}", table.render());
        }
        drop(writer);
        let result = String::from_utf8(buff).map_err(|error| error!("Failed to format maker orders table: {error}"));
        result
    }

    fn format_taker_orders_table(taker_orders: &HashMap<Uuid, TakerOrderForRpc>) -> Result<String, ()> {
        let mut buff = Vec::new();
        let mut writer: Box<dyn Write> = Box::new(&mut buff);

        if taker_orders.is_empty() {
            writeln_field!(writer, "Taker orders", "empty", COMMON_INDENT);
        } else {
            writeln_field!(writer, "Taker orders", "", COMMON_INDENT);
            let mut table = TermTable::new();
            table.style = TableStyle::thin();
            table.add_row(ResponseHandlerImpl::taker_order_header_row());
            for (_, taker_order) in taker_orders.iter().sorted_by_key(|(uuid, _)| *uuid) {
                for row in ResponseHandlerImpl::taker_order_row(taker_order)? {
                    table.add_row(row);
                }
            }
            write_safe_io!(writer, "{}", table.render());
        }
        drop(writer);
        let result = String::from_utf8(buff).map_err(|error| error!("Failed to format maker orders table: {error}"));
        result
    }

    fn maker_order_header_row() -> Row<'static> {
        Row::new(vec![
            TableCell::new("base,rel"),
            TableCell::new("price"),
            TableCell::new("uuid"),
            TableCell::new("created at,\nupdated at"),
            TableCell::new("min base vol,\nmax base vol"),
            TableCell::new("cancellable"),
            TableCell::new("available\namount"),
            TableCell::new("swaps"),
            TableCell::new("conf_settings"),
            TableCell::new("history changes"),
        ])
    }

    fn maker_order_row(maker_order: &MakerOrderForMyOrdersRpc) -> Result<Vec<Row>, ()> {
        let order = &maker_order.order;
        let mut rows = vec![Row::new(vec![
            TableCell::new(format!("{},{}", order.base, order.rel)),
            TableCell::new(format_ratio(&order.price_rat, 2, 5)?),
            TableCell::new(order.uuid),
            TableCell::new(format!(
                "{},\n{}",
                format_datetime(order.created_at)?,
                order.updated_at.map_or("".to_string(), |value| format_datetime(value)
                    .unwrap_or("error".to_string()))
            )),
            TableCell::new(format!(
                "{},\n{}",
                format_ratio(&order.min_base_vol_rat, 2, 5)?,
                format_ratio(&order.max_base_vol_rat, 2, 5)?
            )),
            TableCell::new(maker_order.cancellable),
            TableCell::new(format_ratio(&maker_order.available_amount, 2, 5)?),
            TableCell::new(if order.started_swaps.is_empty() {
                "empty".to_string()
            } else {
                format!("{}", order.started_swaps.iter().join(",\n"))
            }),
            TableCell::new(
                order
                    .conf_settings
                    .map_or_else(|| "none".to_string(), |value| format_confirmation_settings(&value)),
            ),
            TableCell::new(order.changes_history.as_ref().map_or_else(
                || "none".to_string(),
                |val| {
                    val.iter()
                        .map(|val| format_historical_changes(val, "\n").unwrap_or_else(|_| "error".into()))
                        .join(",\n")
                },
            )),
        ])];

        if order.matches.is_empty() {
            return Ok(rows);
        }
        rows.push(Row::new(vec![TableCell::new_with_col_span("matches", 10)]));
        for (uuid, m) in &order.matches {
            let mut matches_str = Vec::new();
            let mut bbox: Box<dyn Write> = Box::new(&mut matches_str);
            Self::write_maker_match(bbox.as_mut(), &uuid, &m).unwrap();
            drop(bbox);
            rows.push(Row::new(vec![TableCell::new_with_col_span(
                String::from_utf8(matches_str).unwrap(),
                10,
            )]));
        }
        Ok(rows)
    }

    fn print_taker_order(&self, writer: &mut dyn Write, taker_status: &TakerOrderForRpc) -> Result<(), ()> {
        let req = &taker_status.request;

        writeln_field!(writer, "uuid", req.uuid, COMMON_INDENT);
        write_base_rel!(writer, req, COMMON_INDENT);
        writeln_field!(writer, "req.action", req.action, COMMON_INDENT);
        writeln_field!(
            writer,
            "req.(sender, dest)",
            format!("{}, {}", req.sender_pubkey, req.dest_pub_key),
            COMMON_INDENT
        );
        writeln_field!(
            writer,
            "req.match_by",
            format_match_by(&req.match_by, "\n"),
            COMMON_INDENT
        );
        write_confirmation_settings!(writer, req, COMMON_INDENT);
        writeln_field!(
            writer,
            "created_at",
            format_datetime(taker_status.created_at)?,
            COMMON_INDENT
        );
        writeln_field!(writer, "order_type", taker_status.order_type, COMMON_INDENT);
        writeln_field!(writer, "cancellable", taker_status.cancellable, COMMON_INDENT);
        write_field_option!(
            writer,
            "base_ob_ticker",
            taker_status.base_orderbook_ticker,
            COMMON_INDENT
        );
        write_field_option!(
            writer,
            "rel_ob_ticker",
            taker_status.rel_orderbook_ticker,
            COMMON_INDENT
        );
        Self::write_taker_matches(writer, &taker_status.matches)?;
        Ok(())
    }

    fn write_taker_matches(writer: &mut dyn Write, matches: &HashMap<Uuid, TakerMatchForRpc>) -> Result<(), ()> {
        if matches.is_empty() {
            //writeln_field!(writer, "matches", "empty", COMMON_INDENT);
            return Ok(());
        }
        writeln_field!(writer, "matches", "", COMMON_INDENT);
        for (uuid, m) in matches {
            Self::write_taker_match(writer, uuid, m)?;
        }
        Ok(())
    }

    fn write_taker_match(writer: &mut dyn Write, uuid: &Uuid, m: &TakerMatchForRpc) -> Result<(), ()> {
        let (reserved, connect, connected) = (&m.reserved, &m.connect, &m.connected);
        writeln_field!(writer, "uuid", uuid, NESTED_INDENT);
        Self::write_maker_reserved_for_rpc(writer, reserved);
        writeln_field!(writer, "last_updated", m.last_updated, NESTED_INDENT);
        write_connected!(writer, connect, NESTED_INDENT);
        if let Some(ref connected) = connected {
            write_connected!(writer, connected, NESTED_INDENT);
        }
        Ok(())
    }

    fn write_maker_reserved_for_rpc(writer: &mut dyn Write, reserved: &MakerReservedForRpc) {
        write_base_rel!(writer, reserved, NESTED_INDENT);
        writeln_field!(
            writer,
            "reserved.(taker, maker)",
            format!("{},{}", reserved.taker_order_uuid, reserved.maker_order_uuid),
            NESTED_INDENT
        );
        writeln_field!(
            writer,
            "reserved.(sender, dest)",
            format!("{},{}", reserved.sender_pubkey, reserved.dest_pub_key),
            NESTED_INDENT
        );
        write_confirmation_settings!(writer, reserved, NESTED_INDENT);
    }
}

mod macros {
    #[macro_export]
    macro_rules! writeln_field {
        ($writer:ident, $name:expr, $value:expr, $width:expr) => {
            writeln_safe_io!($writer, "{:>width$}: {}", $name, $value, width = $width);
        };
    }

    #[macro_export]
    macro_rules! write_field {
        ($writer:ident, $name:expr, $value:expr, $width:expr) => {
            write_safe_io!($writer, "{:>width$}: {}", $name, $value, width = $width);
        };
    }

    #[macro_export]
    macro_rules! write_field_option {
        ($writer:ident, $name:expr, $value:expr, $width:expr) => {
            if let Some(ref value) = $value {
                writeln_safe_io!($writer, "{:>width$}: {}", $name, value, width = $width);
            }
        };
    }

    #[macro_export]
    macro_rules! write_confirmation_settings {
        ($writer:ident, $host:ident, $width:ident) => {
            if $host.conf_settings.is_some() {
                let output = format_confirmation_settings($host.conf_settings.as_ref().unwrap());
                writeln_field!(
                    $writer,
                    concat!(stringify!($host), ".conf_settings"),
                    output,
                    $width
                );
            }
        };
    }

    #[macro_export]
    macro_rules! write_base_rel {
        ($writer:ident, $host:expr, $width:ident) => {
            writeln_field!(
                $writer,
                concat!(stringify!($host), ".(base,rel)"),
                format!(
                    "{}({}), {}({})",
                    $host.base, $host.base_amount, $host.rel, $host.rel_amount
                ),
                $width
            );
        };
    }

    #[macro_export]
    macro_rules! write_connected {
        ($writer:ident, $connected:expr, $width:ident) => {
            writeln_field!(
                $writer,
                concat!(stringify!($connected), ".(taker,maker)"),
                format!("{},{}", $connected.taker_order_uuid, $connected.maker_order_uuid),
                $width
            );
            writeln_field!(
                $writer,
                concat!(stringify!($connected), ".(sender, dest)"),
                format!("{},{}", $connected.sender_pubkey, $connected.dest_pub_key),
                $width
            );
        };
    }

    pub use {write_base_rel, write_confirmation_settings, write_connected, write_field, writeln_field};
}

use crate::write_field_option;
use macros::{write_base_rel, write_confirmation_settings, write_connected, write_field, writeln_field};

fn format_match_by(match_by: &MatchBy, delimiter: &str) -> String {
    match match_by {
        MatchBy::Any => "Any".to_string(),
        MatchBy::Orders(orders) => orders.iter().sorted().join(delimiter),
        MatchBy::Pubkeys(pubkeys) => pubkeys.iter().sorted().join(delimiter),
    }
}

fn format_confirmation_settings(settings: &OrderConfirmationsSettings) -> String {
    format!(
        "{},{}:{},{}",
        settings.base_confs, settings.base_nota, settings.rel_confs, settings.rel_nota
    )
}

fn format_datetime(datetime: u64) -> Result<String, ()> {
    let datetime = Utc
        .timestamp_opt((datetime / 1000) as i64, 0)
        .single()
        .ok_or_else(|| error!("Failed to get datetime formatted datetime"))?;
    Ok(format!("{}", datetime.format("%y-%m-%d %H:%M:%S")))
}

fn format_ratio<T: ToPrimitive>(rational: &T, min_fract: usize, max_fract: usize) -> Result<String, ()> {
    Ok(SmartFractionFmt::new(
        min_fract,
        max_fract,
        rational.to_f64().ok_or_else(|| error!("Failed to convert price_rat"))?,
    )
    .map_err(|_| error!("Failed to create smart_fraction_fmt"))?
    .to_string())
}

fn format_historical_changes(historical_order: &HistoricalOrder, delimiter: &str) -> Result<String, ()> {
    let mut result = vec![];

    if let Some(ref min_base_vol) = historical_order.min_base_vol {
        result.push(format!("min_base_vol: {}", format_ratio(min_base_vol, 2, 5)?,))
    }
    if let Some(ref max_base_vol) = historical_order.max_base_vol {
        result.push(format!("max_base_vol: {}", format_ratio(max_base_vol, 2, 5)?,))
    }
    if let Some(ref price) = historical_order.price {
        result.push(format!("price: {}", format_ratio(price, 2, 5)?));
    }
    if let Some(updated_at) = historical_order.updated_at {
        result.push(format!("updated_at: {}", format_datetime(updated_at)?));
    }
    if let Some(ref conf_settings) = historical_order.conf_settings {
        result.push(format!(
            "conf_settings: {}",
            format_confirmation_settings(conf_settings),
        ));
    }
    Ok(result.join(delimiter))
}
