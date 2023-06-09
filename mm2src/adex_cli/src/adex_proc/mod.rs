mod adex_proc_impl;
mod command;
mod response_handler;
mod smart_fraction_fmt;

pub(crate) use adex_proc_impl::AdexProc;
pub(crate) use response_handler::{ResponseHandler, ResponseHandlerImpl};
pub(crate) use smart_fraction_fmt::SmarFractPrecision;

#[derive(Clone)]
pub(crate) struct OrderbookConfig {
    pub uuids: bool,
    pub min_volume: bool,
    pub max_volume: bool,
    pub publics: bool,
    pub address: bool,
    pub age: bool,
    pub conf_settings: bool,
    pub asks_limit: Option<usize>,
    pub bids_limit: Option<usize>,
}
