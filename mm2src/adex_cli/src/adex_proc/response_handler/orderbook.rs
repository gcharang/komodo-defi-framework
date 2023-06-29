use anyhow::Result;
use itertools::Itertools;
use std::cell::RefMut;
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::io::Write;

use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};
use mm2_number::bigdecimal::ToPrimitive;
use mm2_rpc::data::legacy::{AggregatedOrderbookEntry, OrderbookResponse};

use super::formatters::format_confirmation_settings;
use super::smart_fraction_fmt::{SmartFractPrecision, SmartFractionFmt};
use crate::adex_config::AdexConfig;

pub(crate) struct OrderbookSettings {
    pub(crate) uuids: bool,
    pub(crate) min_volume: bool,
    pub(crate) max_volume: bool,
    pub(crate) publics: bool,
    pub(crate) address: bool,
    pub(crate) age: bool,
    pub(crate) conf_settings: bool,
    pub(crate) asks_limit: Option<usize>,
    pub(crate) bids_limit: Option<usize>,
}

pub(super) fn on_orderbook_response<Cfg: AdexConfig + 'static>(
    mut writer: RefMut<'_, dyn Write>,
    response: OrderbookResponse,
    config: &Cfg,
    settings: OrderbookSettings,
) -> Result<()> {
    let base_vol_head = format!("Volume: {}", response.base);
    let rel_price_head = format!("Price: {}", response.rel);
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
            &settings
        )
    );

    let price_prec = config.orderbook_price_precision();
    let vol_prec = config.orderbook_volume_precision();

    if response.asks.is_empty() {
        writeln_safe_io!(
            writer,
            "{}",
            AskBidRow::new("", "No asks found", "", "", "", "", "", "", "", &settings)
        );
    } else {
        let skip = response
            .asks
            .len()
            .checked_sub(settings.asks_limit.unwrap_or(usize::MAX))
            .unwrap_or_default();

        response
            .asks
            .iter()
            .sorted_by(cmp_asks)
            .skip(skip)
            .map(|entry| AskBidRow::from_orderbook_entry(entry, vol_prec, price_prec, &settings))
            .for_each(|row: AskBidRow| writeln_safe_io!(writer, "{}", row));
    }
    writeln_safe_io!(writer, "{}", AskBidRow::new_delimiter(&settings));

    if response.bids.is_empty() {
        writeln_safe_io!(
            writer,
            "{}",
            AskBidRow::new("", "No bids found", "", "", "", "", "", "", "", &settings)
        );
    } else {
        response
            .bids
            .iter()
            .sorted_by(cmp_bids)
            .take(settings.bids_limit.unwrap_or(usize::MAX))
            .map(|entry| AskBidRow::from_orderbook_entry(entry, vol_prec, price_prec, &settings))
            .for_each(|row: AskBidRow| writeln_safe_io!(writer, "{}", row));
    }
    Ok(())
}

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
    config: &'a OrderbookSettings,
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
        config: &'a OrderbookSettings,
    ) -> Self {
        Self {
            is_mine: AskBidRowVal::Value(String::new()),
            volume: AskBidRowVal::Value(volume.to_string()),
            price: AskBidRowVal::Value(price.to_string()),
            uuid: AskBidRowVal::Value(uuid.to_string()),
            min_volume: AskBidRowVal::Value(min_volume.to_string()),
            max_volume: AskBidRowVal::Value(max_volume.to_string()),
            age: AskBidRowVal::Value(age.to_string()),
            public: AskBidRowVal::Value(public.to_string()),
            address: AskBidRowVal::Value(address.to_string()),
            conf_settings: AskBidRowVal::Value(conf_settings.to_string()),
            config,
        }
    }

    fn new_delimiter(config: &'a OrderbookSettings) -> Self {
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
        vol_prec: &SmartFractPrecision,
        price_prec: &SmartFractPrecision,
        settings: &'a OrderbookSettings,
    ) -> Self {
        AskBidRow {
            is_mine: AskBidRowVal::Value((if entry.entry.is_mine { "*" } else { "" }).to_string()),
            volume: AskBidRowVal::Value(
                SmartFractionFmt::new(vol_prec, entry.entry.base_max_volume.base_max_volume.to_f64().unwrap())
                    .expect("volume smart fraction should be constructed properly")
                    .to_string(),
            ),
            price: AskBidRowVal::Value(
                SmartFractionFmt::new(price_prec, entry.entry.price.to_f64().unwrap())
                    .expect("price smart fraction should be constructed properly")
                    .to_string(),
            ),
            uuid: AskBidRowVal::Value(entry.entry.uuid.to_string()),
            min_volume: AskBidRowVal::Value(
                SmartFractionFmt::new(vol_prec, entry.entry.min_volume.to_f64().unwrap())
                    .expect("min_volume smart fraction should be constructed properly")
                    .to_string(),
            ),
            max_volume: AskBidRowVal::Value(
                SmartFractionFmt::new(vol_prec, entry.entry.max_volume.to_f64().unwrap())
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
                    .as_ref()
                    .map_or("none".to_string(), format_confirmation_settings),
            ),
            config: settings, //TODO: @rozhkovdmitrii
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
