mod orderbook;
mod smart_fraction_fmt;

use chrono::{TimeZone, Utc};
use cli_table::{format::{Border, Separator},
                Table, WithTitle};
use common::io::{write_safe_io, writeln_safe_io, WriteSafeIO};
use itertools::Itertools;
use log::{error, info};
use mm2_number::bigdecimal::ToPrimitive;
use mm2_number::BigRational;
use mm2_rpc_data::legacy::{BalanceResponse, CancelAllOrdersResponse, CoinInitResponse, GetEnabledResponse,
                           HistoricalOrder, MakerMatchForRpc, MakerOrderForMyOrdersRpc, MakerReservedForRpc, MatchBy,
                           Mm2RpcResult, MmVersionResponse, MyOrdersResponse, OrderConfirmationsSettings,
                           OrderStatusResponse, OrderbookResponse, SellBuyResponse, Status, TakerMatchForRpc,
                           TakerOrderForRpc};
use mm2_rpc_data::version2::BestOrdersV2Response;
use serde_json::Value as Json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::Write;
use std::ops::DerefMut;
use uuid::Uuid;

use super::OrderbookConfig;
use crate::adex_config::AdexConfig;
use crate::adex_proc::response_handler::smart_fraction_fmt::SmartFractionFmt;

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
            .map(SimpleCliTable::from_pair)
            .for_each(|value| writeln_safe_io!(writer, "{}: {:?}", value.key, value.value));
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
            write_field!(writer, "Original tickers", "", 0);
            for (coin, ticker) in best_orders.original_tickers {
                write_field!(writer, coin, ticker.iter().join(","), 8);
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

        if my_orders.taker_orders.is_empty() {
            write_field!(writer, "Taker orders", "empty", COMMON_INDENT);
        } else {
            write_field!(writer, "Taker orders", "", COMMON_INDENT);
            for taker_order in my_orders.taker_orders.values() {
                self.print_taker_order(writer, taker_order)?
            }
        }

        if my_orders.maker_orders.is_empty() {
            write_field!(writer, "Maker orders", "empty", COMMON_INDENT);
        } else {
            write_field!(writer, "Maker orders", "", COMMON_INDENT);
            for maker_order in my_orders.maker_orders.values() {
                self.print_maker_order(writer, maker_order)?
            }
        }

        Ok(())
    }
}

impl ResponseHandlerImpl<'_> {
    fn print_maker_order(&self, writer: &mut dyn Write, maker_status: &MakerOrderForMyOrdersRpc) -> Result<(), ()> {
        let order = &maker_status.order;
        write_field!(writer, "base", order.base, COMMON_INDENT);
        write_field!(writer, "rel", order.rel, COMMON_INDENT);
        write_field!(writer, "price", format_ratio(&order.price_rat)?, COMMON_INDENT);
        write_field!(writer, "uuid", order.uuid, COMMON_INDENT);
        write_field!(writer, "created at", format_datetime(order.created_at)?, COMMON_INDENT);
        if let Some(updated_at) = order.updated_at {
            write_field!(writer, "updated at", format_datetime(updated_at)?, COMMON_INDENT);
        }
        write_field!(
            writer,
            "max_base_vol",
            format_ratio(&order.max_base_vol_rat)?,
            COMMON_INDENT
        );
        write_field!(
            writer,
            "min_base_vol",
            format_ratio(&order.min_base_vol_rat)?,
            COMMON_INDENT
        );
        write_field!(
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
            write_field!(
                writer,
                "conf_settings",
                format_confirmation_settings(conf_settings),
                COMMON_INDENT
            );
        }
        if let Some(ref changes_history) = order.changes_history {
            write_field!(
                writer,
                "changes_history",
                changes_history
                    .iter()
                    .map(|val| format_historical_order(val).unwrap_or_else(|_| "error".into()))
                    .join(", "),
                COMMON_INDENT
            );
        }

        write_field!(writer, "cancellable", maker_status.cancellable, COMMON_INDENT);
        write_field!(writer, "available_amount", maker_status.available_amount, COMMON_INDENT);

        Self::write_maker_matches(writer, &order.matches)?;
        writeln_safe_io!(writer, "");
        Ok(())
    }

    fn write_maker_matches(writer: &mut dyn Write, matches: &HashMap<Uuid, MakerMatchForRpc>) -> Result<(), ()> {
        if matches.is_empty() {
            write_field!(writer, "matches", "empty", COMMON_INDENT);
            return Ok(());
        }
        write_field!(writer, "matches", "", COMMON_INDENT);
        for (uid, m) in matches {
            let (req, reserved, connect, connected) = (&m.request, &m.reserved, &m.connect, &m.connected);
            write_field!(writer, "uuid", uid, NESTED_INDENT);
            write_field!(writer, "req.uuid", req.uuid, NESTED_INDENT);
            write_base_rel!(writer, req, NESTED_INDENT);
            write_field!(writer, "req.match_by", format_match_by(&req.match_by), NESTED_INDENT);
            write_field!(writer, "req.action", req.action, NESTED_INDENT);
            write_confirmation_settings!(writer, req, NESTED_INDENT);
            write_field!(
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
        }
        Ok(())
    }

    fn print_taker_order(&self, writer: &mut dyn Write, taker_status: &TakerOrderForRpc) -> Result<(), ()> {
        let req = &taker_status.request;
        write_field!(writer, "uuid", req.uuid, COMMON_INDENT);
        write_base_rel!(writer, req, COMMON_INDENT);
        write_field!(writer, "req.action", req.action, COMMON_INDENT);
        write_field!(
            writer,
            "req.(sender, dest)",
            format!("{}, {}", req.sender_pubkey, req.dest_pub_key),
            COMMON_INDENT
        );
        write_field!(writer, "req.match_by", format_match_by(&req.match_by), COMMON_INDENT);
        write_confirmation_settings!(writer, req, COMMON_INDENT);
        write_field!(
            writer,
            "created_at",
            format_datetime(taker_status.created_at)?,
            COMMON_INDENT
        );
        write_field!(writer, "order_type", taker_status.order_type, COMMON_INDENT);
        write_field!(writer, "cancellable", taker_status.cancellable, COMMON_INDENT);
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
        writeln_safe_io!(writer, "");
        Ok(())
    }

    fn write_taker_matches(writer: &mut dyn Write, matches: &HashMap<Uuid, TakerMatchForRpc>) -> Result<(), ()> {
        if matches.is_empty() {
            write_field!(writer, "matches", "empty", COMMON_INDENT);
            return Ok(());
        }
        write_field!(writer, "matches", "", COMMON_INDENT);
        for (uuid, m) in matches {
            let (reserved, connect, connected) = (&m.reserved, &m.connect, &m.connected);
            write_field!(writer, "uuid", uuid, NESTED_INDENT);
            Self::write_maker_reserved_for_rpc(writer, reserved);
            write_field!(writer, "last_updated", m.last_updated, NESTED_INDENT);
            write_connected!(writer, connect, NESTED_INDENT);
            if let Some(ref connected) = connected {
                write_connected!(writer, connected, NESTED_INDENT);
            }
        }
        Ok(())
    }

    fn write_maker_reserved_for_rpc(writer: &mut dyn Write, reserved: &MakerReservedForRpc) {
        write_base_rel!(writer, reserved, NESTED_INDENT);
        write_field!(
            writer,
            "reserved.(taker, maker)",
            format!("{},{}", reserved.taker_order_uuid, reserved.maker_order_uuid),
            NESTED_INDENT
        );
        write_field!(
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
    macro_rules! write_field {
        ($writer:ident, $name:expr, $value:expr, $width:expr) => {
            writeln_safe_io!($writer, "{:>width$}: {}", $name, $value, width = $width);
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
                write_field!(
                    $writer,
                    concat!(stringify!($host), "conf_settings"),
                    output,
                    $width
                );
            }
        };
    }

    #[macro_export]
    macro_rules! write_base_rel {
        ($writer:ident, $host:expr, $width:ident) => {
            write_field!(
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
            write_field!(
                $writer,
                concat!(stringify!($connected), ".(taker,maker)"),
                format!("{},{}", $connected.taker_order_uuid, $connected.maker_order_uuid),
                $width
            );
            write_field!(
                $writer,
                concat!(stringify!($connected), ".(sender, dest)"),
                format!("{},{}", $connected.sender_pubkey, $connected.dest_pub_key),
                $width
            );
        };
    }

    pub use {write_base_rel, write_confirmation_settings, write_connected, write_field};
}

use crate::write_field_option;
use macros::{write_base_rel, write_confirmation_settings, write_connected, write_field};

struct SimpleCliTable<'a> {
    key: &'a String,
    value: &'a Json,
}

impl<'a> SimpleCliTable<'a> {
    fn from_pair(pair: (&'a String, &'a Json)) -> Self {
        SimpleCliTable {
            key: pair.0,
            value: pair.1,
        }
    }
}

fn format_match_by(match_by: &MatchBy) -> String {
    match match_by {
        MatchBy::Any => "Any".to_string(),
        MatchBy::Orders(orders) => orders.iter().join(", "),
        MatchBy::Pubkeys(pubkeys) => pubkeys.iter().join(", "),
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
    Ok(format!("{}", datetime))
}

fn format_ratio(rational: &BigRational) -> Result<f64, ()> {
    rational.to_f64().ok_or_else(|| error!("Failed to convert price_rat"))
}

fn format_historical_order(historical_order: &HistoricalOrder) -> Result<String, ()> {
    let mut result = String::new();
    if let Some(ref max_base_vol) = historical_order.max_base_vol {
        result += &format!("max_base_vol: {}, ", format_ratio(max_base_vol)?)
    }
    if let Some(ref min_base_vol) = historical_order.min_base_vol {
        result += &format!("min_base_vol: {}, ", format_ratio(min_base_vol)?)
    }
    if let Some(ref price) = historical_order.price {
        result += &format!("price: {}, ", format_ratio(price)?);
    }
    if let Some(updated_at) = historical_order.updated_at {
        result += &format!("updated_at: {}", format_datetime(updated_at)?);
    }
    if let Some(ref conf_settings) = historical_order.conf_settings {
        result += &format!("conf_settings: {}", format_confirmation_settings(conf_settings));
    }
    Ok(result)
}
