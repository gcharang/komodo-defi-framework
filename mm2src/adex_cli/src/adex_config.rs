use directories::ProjectDirs;
use inquire::Password;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use crate::helpers::rewrite_json_file;

const PROJECT_QUALIFIER: &str = "com";
const PROJECT_COMPANY: &str = "komodoplatform";
const PROJECT_APP: &str = "adex-cli";
const ADEX_CFG: &str = "adex_cfg.json";

const PRICE_PRECISION_MIN: usize = 8;
const PRICE_PRECISION_MAX: usize = 8;
const VOLUME_PRECISION_MIN: usize = 2;
const VOLUME_PRECISION_MAX: usize = 5;
const VOLUME_PRECISION: VolumePrecision = (VOLUME_PRECISION_MIN, VOLUME_PRECISION_MAX);
const PRICE_PRECISION: PricePrecision = (PRICE_PRECISION_MIN, PRICE_PRECISION_MAX);

pub(crate) type PricePrecision = (usize, usize);
pub(crate) type VolumePrecision = (usize, usize);

pub fn get_config() {
    let Ok(adex_cfg) = AdexConfigImpl::from_config_path() else { return; };
    info!("{}", adex_cfg)
}

pub fn set_config(set_password: bool, rpc_api_uri: Option<String>) {
    let mut adex_cfg = AdexConfigImpl::from_config_path().unwrap_or_else(|()| AdexConfigImpl::default());
    let mut is_changes_happened = false;
    if set_password {
        let rpc_password = Password::new("Enter RPC API password:")
            .prompt()
            .map(|value| {
                is_changes_happened = true;
                value
            })
            .map_err(|error| error!("Failed to get rpc_api_password: {error}"))
            .ok();
        adex_cfg.set_rpc_password(rpc_password);
    }
    if rpc_api_uri.is_some() {
        adex_cfg.set_rpc_uri(rpc_api_uri);
        is_changes_happened = true;
    }

    if is_changes_happened && adex_cfg.write_to_config_path().is_ok() {
        info!("Configuration has been set");
    } else {
        warn!("Nothing changed");
    }
}

pub(crate) trait AdexConfig {
    fn rpc_password(&self) -> String;
    fn rpc_uri(&self) -> String;
    fn orderbook_price_precision(&self) -> &PricePrecision;
    fn orderbook_volume_precision(&self) -> &VolumePrecision;
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub(crate) struct AdexConfigImpl {
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc_uri: Option<String>,
}

impl AdexConfig for AdexConfigImpl {
    fn rpc_password(&self) -> String { self.rpc_password.as_ref().expect("No rpc_password in config").clone() }
    fn rpc_uri(&self) -> String { self.rpc_uri.as_ref().expect("No rpc_uri in config").clone() }
    fn orderbook_price_precision(&self) -> &PricePrecision { &PRICE_PRECISION }
    fn orderbook_volume_precision(&self) -> &VolumePrecision { &VOLUME_PRECISION }
}

impl Display for AdexConfigImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.is_set() {
            return writeln!(f, "adex configuration is not set");
        }
        if let Some(rpc_api_uri) = &self.rpc_uri {
            writeln!(f, "mm2 RPC URL: {}", rpc_api_uri)?
        };

        if self.rpc_password.is_some() {
            writeln!(f, "mm2 RPC password: *************")?
        }
        Ok(())
    }
}

impl AdexConfigImpl {
    #[cfg(test)]
    pub fn new(rpc_password: &str, rpc_uri: &str) -> Self {
        Self {
            rpc_password: Some(rpc_password.to_string()),
            rpc_uri: Some(rpc_uri.to_string()),
        }
    }

    #[cfg(not(test))]
    pub fn read_config() -> Result<AdexConfigImpl, ()> {
        let config = AdexConfigImpl::from_config_path().map_err(|_| error!("Failed to get adex_config"))?;
        match config {
            config @ AdexConfigImpl {
                rpc_password: Some(_),
                rpc_uri: Some(_),
            } => Ok(config),
            _ => {
                warn!("Failed to process, adex_config is not fully set");
                Err(())
            },
        }
    }

    fn is_set(&self) -> bool { self.rpc_uri.is_some() && self.rpc_password.is_some() }

    pub fn get_config_dir() -> Result<PathBuf, ()> {
        let project_dirs = ProjectDirs::from(PROJECT_QUALIFIER, PROJECT_COMPANY, PROJECT_APP)
            .ok_or_else(|| error!("Failed to get project_dirs"))?;
        let config_path: PathBuf = project_dirs.config_dir().into();
        fs::create_dir_all(&config_path)
            .map_err(|error| error!("Failed to create config_dir: {config_path:?}, error: {error}"))?;
        Ok(config_path)
    }

    fn get_config_path() -> Result<PathBuf, ()> {
        let mut config_path = Self::get_config_dir()?;
        config_path.push(ADEX_CFG);
        Ok(config_path)
    }

    fn from_config_path() -> Result<AdexConfigImpl, ()> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            warn!("Config is not set");
            return Err(());
        }
        Self::read_from(&config_path)
    }

    fn write_to_config_path(&self) -> Result<(), ()> {
        let config_path = Self::get_config_path()?;
        self.write_to(&config_path)
    }

    fn read_from(cfg_path: &Path) -> Result<AdexConfigImpl, ()> {
        let adex_path_str = cfg_path.to_str().unwrap_or("Undefined");
        let adex_cfg_file = fs::File::open(cfg_path).map_err(|error| {
            error!("Failed to open: {adex_path_str}, error: {error}");
        })?;

        serde_json::from_reader(adex_cfg_file)
            .map_err(|error| error!("Failed to read adex_cfg to read from: {adex_path_str}, error: {error}"))
    }

    fn write_to(&self, cfg_path: &Path) -> Result<(), ()> {
        let adex_path_str = cfg_path
            .to_str()
            .ok_or_else(|| error!("Failed to get cfg_path as str"))?;
        rewrite_json_file(self, adex_path_str)
    }

    fn set_rpc_password(&mut self, rpc_password: Option<String>) { self.rpc_password = rpc_password; }

    fn set_rpc_uri(&mut self, rpc_uri: Option<String>) { self.rpc_uri = rpc_uri; }
}
