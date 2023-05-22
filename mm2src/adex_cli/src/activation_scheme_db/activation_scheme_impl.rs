use log::{debug, error, warn};
use serde_json::{json, Value as Json};
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

    pub fn get_activation_method(&self, coin: &str) -> Option<Json> {
        let Some(Json::Object(object)) = self.scheme.get(coin) else { return None };
        let mut copy = json!({});
        for (k, v) in object.iter() {
            // WORKAROUND: serde_json::Value does not support removing key
            if *k == "userpass" {
                continue;
            }
            copy[k] = v.clone();
        }
        Some(copy)
    }

    fn init(&mut self) -> Result<(), ()> {
        let mut results: Vec<Json> = Self::load_json_file()?;
        self.scheme = results.iter_mut().map(Self::get_coin_pair).collect();
        Ok(())
    }

    fn get_coin_pair(element: &mut Json) -> (String, Json) {
        let presence = element.to_string();
        let Ok(result) = Self::get_coin_pair_impl(element) else {
            warn!("Failed to process: {presence}");
            return ("".to_string(), Json::Null)
        };
        result
    }

    fn get_coin_pair_impl(element: &mut Json) -> Result<(String, Json), ()> {
        let command = element.get_mut("command").ok_or(())?.take();
        let coin = element.get("coin").ok_or(())?.as_str().ok_or(())?.to_string();
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
