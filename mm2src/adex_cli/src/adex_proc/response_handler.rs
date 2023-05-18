use itertools::Itertools;
use log::{error, info};
use mm2_rpc_data::legacy::{BalanceResponse, CoinInitResponse, GetEnabledResponse, Mm2RpcResult, MmVersionResponse,
                           OrderbookResponse, SellBuyResponse, Status};
use serde_json::Value as Json;
use std::cell::RefCell;
use std::fmt::Debug;
use std::io::Write;

use super::OrderbookConfig;
use crate::adex_config::AdexConfig;
use common::io::{write_safe_io, writeln_safe_io, WriteSafeIO};

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
        if response.mature_confirmations.is_some() {
            writeln_safe_io!(
                writer,
                "mature_confirmations: {}",
                response.mature_confirmations.unwrap()
            );
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
        writeln_safe_io!(self.writer.borrow_mut(), "Buy order uuid: {}", response.request.uuid);
        Ok(())
    }

    fn on_stop_response(&self, response: &Mm2RpcResult<Status>) -> Result<(), ()> {
        writeln_safe_io!(self.writer.borrow_mut(), "Service stopped: {}", response.result);
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

mod orderbook {
    use mm2_number::bigdecimal::ToPrimitive;
    use mm2_rpc_data::legacy::AggregatedOrderbookEntry;
    use std::cmp::Ordering;
    use std::fmt::{Display, Formatter};

    use super::super::{smart_fraction_fmt::SmartFractionFmt, OrderbookConfig};
    use crate::adex_config::{PricePrecision, VolumePrecision};

    pub fn cmp_bids(left: &&AggregatedOrderbookEntry, right: &&AggregatedOrderbookEntry) -> Ordering {
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

    pub fn cmp_asks(left: &&AggregatedOrderbookEntry, right: &&AggregatedOrderbookEntry) -> Ordering {
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

    pub struct AskBidRow<'a> {
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
        pub(crate) fn new(
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

        pub(crate) fn new_delimiter(config: &'a OrderbookConfig) -> Self {
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

        pub(crate) fn from_orderbook_entry(
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
                conf_settings: AskBidRowVal::Value(entry.entry.conf_settings.map_or("none".into(), |settings| {
                    format!(
                        "{},{}:{},{}",
                        settings.base_confs, settings.base_nota, settings.rel_confs, settings.rel_nota
                    )
                })),
                config,
            }
        }
    }

    impl Display for AskBidRow<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            macro_rules! write_ask_bid_row {
                ($value: expr, $width: expr, $alignment: literal) => {
                    if let AskBidRowVal::Value(value) = &$value {
                        write!(
                            f,
                            concat!("{:", $alignment, "width$} "),
                            value,
                            width = $width
                        )?;
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
}
