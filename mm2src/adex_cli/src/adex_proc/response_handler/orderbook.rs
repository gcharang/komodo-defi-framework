use mm2_number::bigdecimal::ToPrimitive;
use mm2_rpc_data::legacy::AggregatedOrderbookEntry;
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

use super::{format_confirmation_settings, smart_fraction_fmt::SmartFractionFmt, OrderbookConfig};
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
