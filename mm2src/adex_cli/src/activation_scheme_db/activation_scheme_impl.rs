use log::{debug, error, warn};
use serde_json::Value as Json;
use std::collections::HashMap;

use super::init_activation_scheme::get_activation_scheme_path;
use crate::helpers::read_json_file;

pub struct ActivationScheme {
    scheme: HashMap<String, Json>,
}

impl ActivationScheme {
    fn new() -> Self {
        Self {
            scheme: HashMap::<String, Json>::new(),
        }
    }

    pub fn get_activation_method(&self, coin: &str) -> Option<Json> { self.scheme.get(coin).cloned() }

    fn init(&mut self) -> Result<(), ()> {
        let mut results: Vec<Json> = Self::load_json_file()?;
        self.scheme = results.iter_mut().filter_map(Self::get_coin_pair).collect();
        Ok(())
    }

    fn get_coin_pair(element: &mut Json) -> Option<(String, Json)> {
        Self::get_coin_pair_impl(element)
            .map_err(|_| warn!("Failed to get coin pair from: {}", element.to_string()))
            .ok()
    }

    fn get_coin_pair_impl(element: &mut Json) -> Result<(String, Json), ()> {
        let coin = element.get("coin").ok_or(())?.as_str().ok_or(())?.to_string();
        let mut command = element.get_mut("command").ok_or(())?.take();
        command
            .as_object_mut()
            .ok_or_else(|| error!("Failed to get coin pair, command is not object"))?
            .remove("userpass");
        Ok((coin, command))
    }

    fn load_json_file() -> Result<Vec<Json>, ()> {
        let activation_scheme_path = get_activation_scheme_path()?;
        debug!("Start reading activation_scheme from: {activation_scheme_path:?}");

        let mut activation_scheme: Json = read_json_file(&activation_scheme_path)?;

        let Json::Array(results) = activation_scheme
            .get_mut("results")
            .ok_or_else(|| error!("Failed to load activation scheme json file, no results section"))?
            .take()
        else {
            error!("Failed to load activation scheme json file, wrong format");
            return Err(());
        };
        Ok(results)
    }
}

pub(crate) fn get_activation_scheme() -> Result<ActivationScheme, ()> {
    let mut activation_scheme = ActivationScheme::new();
    activation_scheme.init()?;
    Ok(activation_scheme)
}
