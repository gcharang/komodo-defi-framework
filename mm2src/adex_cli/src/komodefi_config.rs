use anyhow::{anyhow, bail, Result};
use common::log::{error, info, warn};
use directories::ProjectDirs;
use inquire::Password;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use crate::helpers::rewrite_json_file;
#[cfg(unix)] use crate::helpers::set_file_permissions;
use crate::komodefi_proc::SmartFractPrecision;
use crate::logging::{error_anyhow, warn_bail};

const PROJECT_QUALIFIER: &str = "com";
const PROJECT_COMPANY: &str = "komodoplatform";
const PROJECT_APP: &str = "komodefi-cli";
const KOMODEFI_CFG: &str = "komodefi_cfg.json";

const PRICE_PRECISION_MIN: usize = 8;
const PRICE_PRECISION_MAX: usize = 8;
const VOLUME_PRECISION_MIN: usize = 2;
const VOLUME_PRECISION_MAX: usize = 5;
const VOLUME_PRECISION: SmartFractPrecision = (VOLUME_PRECISION_MIN, VOLUME_PRECISION_MAX);
const PRICE_PRECISION: SmartFractPrecision = (PRICE_PRECISION_MIN, PRICE_PRECISION_MAX);
#[cfg(unix)]
const CFG_FILE_PERM_MODE: u32 = 0o660;

pub(super) fn get_config() {
    let Ok(komodefi_cfg) = KomodefiConfigImpl::from_config_path() else { return; };
    info!("{}", komodefi_cfg)
}

pub(super) fn set_config(set_password: bool, rpc_api_uri: Option<String>) -> Result<()> {
    assert!(set_password || rpc_api_uri.is_some());
    let mut komodefi_cfg = KomodefiConfigImpl::from_config_path().unwrap_or_else(|_| KomodefiConfigImpl::default());

    if set_password {
        let rpc_password = Password::new("Enter RPC API password:")
            .prompt()
            .map_err(|error| error_anyhow!("Failed to get rpc_api_password: {error}"))?;
        komodefi_cfg.set_rpc_password(rpc_password);
    }

    if let Some(rpc_api_uri) = rpc_api_uri {
        komodefi_cfg.set_rpc_uri(rpc_api_uri);
    }

    komodefi_cfg.write_to_config_path()?;
    info!("Configuration has been set");

    Ok(())
}

pub(super) trait KomodefiConfig {
    fn rpc_password(&self) -> Option<String>;
    fn rpc_uri(&self) -> Option<String>;
    fn orderbook_price_precision(&self) -> &SmartFractPrecision;
    fn orderbook_volume_precision(&self) -> &SmartFractPrecision;
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct KomodefiConfigImpl {
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc_uri: Option<String>,
}

impl KomodefiConfig for KomodefiConfigImpl {
    fn rpc_password(&self) -> Option<String> { self.rpc_password.clone() }
    fn rpc_uri(&self) -> Option<String> { self.rpc_uri.clone() }
    fn orderbook_price_precision(&self) -> &SmartFractPrecision { &PRICE_PRECISION }
    fn orderbook_volume_precision(&self) -> &SmartFractPrecision { &VOLUME_PRECISION }
}

impl Display for KomodefiConfigImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.is_set() {
            return writeln!(f, "komodefi configuration is not set");
        }
        writeln!(
            f,
            "mm2 RPC URL: {}",
            self.rpc_uri.as_ref().expect("Expected rpc_uri is set")
        )?;
        writeln!(f, "mm2 RPC password: *************")?;
        Ok(())
    }
}

impl KomodefiConfigImpl {
    #[cfg(test)]
    pub(super) fn new(rpc_password: &str, rpc_uri: &str) -> Self {
        Self {
            rpc_password: Some(rpc_password.to_string()),
            rpc_uri: Some(rpc_uri.to_string()),
        }
    }

    #[cfg(not(test))]
    pub(super) fn read_config() -> Result<KomodefiConfigImpl> {
        let config = KomodefiConfigImpl::from_config_path()?;
        if config.rpc_password.is_none() {
            warn!("Configuration is not complete, no rpc_password in there");
        }
        if config.rpc_uri.is_none() {
            warn!("Configuration is not complete, no rpc_uri in there");
        }
        Ok(config)
    }

    fn is_set(&self) -> bool { self.rpc_uri.is_some() && self.rpc_password.is_some() }

    pub(super) fn get_config_dir() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from(PROJECT_QUALIFIER, PROJECT_COMPANY, PROJECT_APP)
            .ok_or_else(|| error_anyhow!("Failed to get project_dirs"))?;
        let config_path: PathBuf = project_dirs.config_dir().into();
        fs::create_dir_all(&config_path)
            .map_err(|error| error_anyhow!("Failed to create config_dir: {config_path:?}, error: {error}"))?;
        Ok(config_path)
    }

    pub(crate) fn get_config_path() -> Result<PathBuf> {
        let config_path = if let Ok(config_path) = std::env::var("KOMODO_CLI_CFG") {
            PathBuf::from(config_path)
        } else {
            let mut config_path = KomodefiConfigImpl::get_config_dir()?;
            config_path.push(KOMODEFI_CFG);
            config_path
        };
        Ok(config_path)
    }

    fn from_config_path() -> Result<KomodefiConfigImpl> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            warn_bail!("Config is not set")
        }
        Self::read_from(&config_path)
    }

    fn write_to_config_path(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        self.write_to(&config_path)
    }

    fn read_from(cfg_path: &Path) -> Result<KomodefiConfigImpl> {
        let komodefi_path_str = cfg_path.to_str().unwrap_or("Undefined");
        let komodefi_cfg_file = fs::File::open(cfg_path)
            .map_err(|error| error_anyhow!("Failed to open: {komodefi_path_str}, error: {error}"))?;

        serde_json::from_reader(komodefi_cfg_file).map_err(|error| {
            error_anyhow!("Failed to read komodefi_cfg to read from: {komodefi_path_str}, error: {error}")
        })
    }

    fn write_to(&self, cfg_path: &Path) -> Result<()> {
        let komodefi_path_str = cfg_path
            .to_str()
            .ok_or_else(|| error_anyhow!("Failed to get cfg_path as str"))?;
        rewrite_json_file(self, komodefi_path_str)?;
        #[cfg(unix)]
        {
            set_file_permissions(komodefi_path_str, CFG_FILE_PERM_MODE)?;
        }
        Ok(())
    }

    fn set_rpc_password(&mut self, rpc_password: String) { self.rpc_password.replace(rpc_password); }

    fn set_rpc_uri(&mut self, rpc_uri: String) { self.rpc_uri.replace(rpc_uri); }
}
