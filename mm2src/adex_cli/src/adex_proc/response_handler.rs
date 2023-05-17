use chrono::{TimeZone, Utc};
use itertools::Itertools;
use log::{error, info};
use mm2_number::bigdecimal::ToPrimitive;
use mm2_number::BigRational;
use mm2_rpc_data::legacy::{AggregatedOrderbookEntry, BalanceResponse, CancelAllOrdersResponse, CoinInitResponse,
                           GetEnabledResponse, HistoricalOrder, MakerMatchForRpc, MakerOrderForMyOrdersRpc,
                           MakerReservedForRpc, MatchBy, Mm2RpcResult, MmVersionResponse, OrderConfirmationsSettings,
                           OrderStatusResponse, OrderbookResponse, SellBuyResponse, Status, TakerMatchForRpc,
                           TakerOrderForRpc};
use serde_json::Value as Json;
use std::cell::{RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::io::Write;
use uuid::Uuid;

use super::smart_fraction_fmt::SmartFractionFmt;
use super::OrderbookConfig;
use crate::adex_config::{AdexConfig, PricePrecision, VolumePrecision};
use common::io::{write_safe_io, writeln_safe_io, WriteSafeIO};

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
}

pub(crate) struct ResponseHandlerImpl<'a> {
    pub writer: RefCell<&'a mut dyn Write>,
}

impl ResponseHandler for ResponseHandlerImpl<'_> {
    fn print_response(&self, result: Json) -> Result<(), ()> {
        let object = result
            .as_object()
            .ok_or_else(|| error!("Failed to cast result as object"))?;

        object
            .iter()
            .map(SimpleCliTable::from_pair)
            .for_each(|value| writeln_safe_io!(self.writer.borrow_mut(), "{}: {:?}", value.key, value.value));
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
            AskBidRow::new(
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
                AskBidRow::new("", "No asks found", "", "", "", "", "", "", "", &otderbook_config)
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
                .sorted_by(cmp_asks)
                .skip(skip)
                .map(|entry| AskBidRow::from_orderbook_entry(entry, vol_prec, price_prec, &otderbook_config))
                .for_each(|row: AskBidRow| writeln_safe_io!(writer, "{}", row));
        }
        writeln_safe_io!(writer, "{}", AskBidRow::new_delimiter(&otderbook_config));

        if orderbook.bids.is_empty() {
            writeln_safe_io!(
                writer,
                "{}",
                AskBidRow::new("", "No bids found", "", "", "", "", "", "", "", &otderbook_config)
            );
        } else {
            orderbook
                .bids
                .iter()
                .sorted_by(cmp_bids)
                .take(otderbook_config.bids_limit.unwrap_or(usize::MAX))
                .map(|entry| AskBidRow::from_orderbook_entry(entry, vol_prec, price_prec, &otderbook_config))
                .for_each(|row: AskBidRow| writeln_safe_io!(writer, "{}", row));
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
        match response {
            OrderStatusResponse::Maker(maker_status) => self.print_maker_status(maker_status)?,
            OrderStatusResponse::Taker(taker_status) => self.print_taker_status(taker_status)?,
        }
        Ok(())
    }
}

impl ResponseHandlerImpl<'_> {
    fn print_maker_status(&self, maker_status: &MakerOrderForMyOrdersRpc) -> Result<(), ()> {
        let order = &maker_status.order;
        let mut writer = self.writer.borrow_mut();
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
            format!("{}", order.started_swaps.iter().join(", ")),
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

        Self::write_maker_matches(&mut writer, &order.matches)?;

        Ok(())
    }

    fn print_taker_status(&self, taker_status: &TakerOrderForRpc) -> Result<(), ()> {
        let mut writer = self.writer.borrow_mut();
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
        Self::write_taker_matches(&mut writer, &taker_status.matches)
    }

    fn write_maker_matches(
        writer: &mut RefMut<&mut dyn Write>,
        matches: &HashMap<Uuid, MakerMatchForRpc>,
    ) -> Result<(), ()> {
        if matches.is_empty() {
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

    fn write_taker_matches(
        writer: &mut RefMut<&mut dyn Write>,
        matches: &HashMap<Uuid, TakerMatchForRpc>,
    ) -> Result<(), ()> {
        if matches.is_empty() {
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

    fn write_maker_reserved_for_rpc(writer: &mut RefMut<&mut dyn Write>, reserved: &MakerReservedForRpc) {
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
        ($writer:ident, $name:expr, $value:expr, $width:ident) => {
            writeln_safe_io!($writer, "{:>width$}: {}", $name, $value, width = $width);
        };
    }
    #[macro_export]
    macro_rules! write_field_option {
        ($writer:ident, $name:expr, $value:expr, $width:ident) => {
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

    pub use {write_confirmation_settings, write_connected, write_field};
}

use crate::{write_base_rel, write_field_option};
use macros::{write_confirmation_settings, write_connected, write_field};

fn cmp_bids(left: &&AggregatedOrderbookEntry, right: &&AggregatedOrderbookEntry) -> Ordering {
    let cmp = left.entry.price.cmp(&right.entry.price).reverse();
    if cmp.is_eq() {
        return left
            .entry
            .base_max_volume
            .base_max_volume
            .cmp(&right.entry.base_max_volume.base_max_volume)
            .reverse();
    }
    cmp
}

fn cmp_asks(left: &&AggregatedOrderbookEntry, right: &&AggregatedOrderbookEntry) -> Ordering {
    let cmp = left.entry.price.cmp(&right.entry.price).reverse();
    if cmp.is_eq() {
        return left
            .entry
            .base_max_volume
            .base_max_volume
            .cmp(&right.entry.base_max_volume.base_max_volume);
    }
    cmp
}

enum AskBidRowVal {
    Value(String),
    Delim,
}

struct AskBidRow<'a> {
    volume: AskBidRowVal,
    price: AskBidRowVal,
    uuid: AskBidRowVal,
    min_volume: AskBidRowVal,
    max_volume: AskBidRowVal,
    age: AskBidRowVal,
    public: AskBidRowVal,
    address: AskBidRowVal,
    is_mine: AskBidRowVal,
    conf_settings: AskBidRowVal,
    config: &'a OrderbookConfig,
}

impl<'a> AskBidRow<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        volume: &str,
        price: &str,
        uuid: &str,
        min_volume: &str,
        max_volume: &str,
        age: &str,
        public: &str,
        address: &str,
        conf_settings: &str,
        config: &'a OrderbookConfig,
    ) -> Self {
        Self {
            is_mine: AskBidRowVal::Value(String::new()),
            volume: AskBidRowVal::Value(volume.into()),
            price: AskBidRowVal::Value(price.into()),
            uuid: AskBidRowVal::Value(uuid.into()),
            min_volume: AskBidRowVal::Value(min_volume.into()),
            max_volume: AskBidRowVal::Value(max_volume.into()),
            age: AskBidRowVal::Value(age.into()),
            public: AskBidRowVal::Value(public.into()),
            address: AskBidRowVal::Value(address.into()),
            conf_settings: AskBidRowVal::Value(conf_settings.into()),
            config,
        }
    }

    fn new_delimiter(config: &'a OrderbookConfig) -> Self {
        Self {
            is_mine: AskBidRowVal::Delim,
            volume: AskBidRowVal::Delim,
            price: AskBidRowVal::Delim,
            uuid: AskBidRowVal::Delim,
            min_volume: AskBidRowVal::Delim,
            max_volume: AskBidRowVal::Delim,
            age: AskBidRowVal::Delim,
            public: AskBidRowVal::Delim,
            address: AskBidRowVal::Delim,
            conf_settings: AskBidRowVal::Delim,
            config,
        }
    }

    fn from_orderbook_entry(
        entry: &AggregatedOrderbookEntry,
        vol_prec: &VolumePrecision,
        price_prec: &PricePrecision,
        config: &'a OrderbookConfig,
    ) -> Self {
        AskBidRow {
            is_mine: AskBidRowVal::Value(if entry.entry.is_mine { "*".into() } else { "".into() }),
            volume: AskBidRowVal::Value(
                SmartFractionFmt::new(
                    vol_prec.0,
                    vol_prec.1,
                    entry.entry.base_max_volume.base_max_volume.to_f64().unwrap(),
                )
                .expect("volume smart fraction should be constructed properly")
                .to_string(),
            ),
            price: AskBidRowVal::Value(
                SmartFractionFmt::new(price_prec.0, price_prec.1, entry.entry.price.to_f64().unwrap())
                    .expect("price smart fraction should be constructed properly")
                    .to_string(),
            ),
            uuid: AskBidRowVal::Value(entry.entry.uuid.to_string()),
            min_volume: AskBidRowVal::Value(
                SmartFractionFmt::new(vol_prec.0, vol_prec.1, entry.entry.min_volume.to_f64().unwrap())
                    .expect("min_volume smart fraction should be constructed properly")
                    .to_string(),
            ),
            max_volume: AskBidRowVal::Value(
                SmartFractionFmt::new(vol_prec.0, vol_prec.1, entry.entry.max_volume.to_f64().unwrap())
                    .expect("max_volume smart fraction should be constructed properly")
                    .to_string(),
            ),
            age: AskBidRowVal::Value(entry.entry.age.to_string()),
            public: AskBidRowVal::Value(entry.entry.pubkey.clone()),
            address: AskBidRowVal::Value(entry.entry.address.clone()),
            conf_settings: AskBidRowVal::Value(
                entry
                    .entry
                    .conf_settings
                    .map_or("none".into(), |settings| format_confirmation_settings(&settings)),
            ),
            config,
        }
    }
}

impl Display for AskBidRow<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        macro_rules! write_ask_bid_row {
            ($value: expr, $width: expr, $alignment: literal) => {
                if let AskBidRowVal::Value(value) = &$value {
                    write!(f, concat!("{:", $alignment, "width$} "), value, width = $width)?;
                } else {
                    write!(f, "{:-<width$} ", "", width = $width)?;
                };
            };
            ($config: expr, $value: expr, $width: expr, $alignment: literal) => {
                if $config {
                    write_ask_bid_row!($value, $width, $alignment);
                }
            };
        }
        write_ask_bid_row!(self.is_mine, 1, "<");
        write_ask_bid_row!(self.volume, 15, ">");
        write_ask_bid_row!(self.price, 13, "<");
        write_ask_bid_row!(self.config.uuids, self.uuid, 36, "<");
        write_ask_bid_row!(self.config.min_volume, self.min_volume, 10, "<");
        write_ask_bid_row!(self.config.max_volume, self.max_volume, 10, "<");
        write_ask_bid_row!(self.config.age, self.age, 10, "<");
        write_ask_bid_row!(self.config.publics, self.public, 66, "<");
        write_ask_bid_row!(self.config.address, self.address, 34, "<");
        write_ask_bid_row!(self.config.conf_settings, self.conf_settings, 24, "<");
        Ok(())
    }
}

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
