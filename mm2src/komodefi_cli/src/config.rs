use crate::cli_cmd_args::prelude::SetConfigArgs;
use crate::helpers::rewrite_json_file;
use crate::komodefi_proc::SmartFractPrecision;
use crate::logging::{error_anyhow, warn_bail};

use anyhow::{anyhow, bail, Result};
use common::log::{error, info, warn};
use directories::ProjectDirs;
use inquire::Password;
use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(unix)] use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use url::{ParseError, Url};

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

pub(super) fn get_config(option: &crate::cli::GetOption) {
    let Ok(komodefi_cfg) = KomodefiConfigImpl::from_config_path() else { return; };
    info!("{}", komodefi_cfg.print_config(option.unhide))
}

pub(super) fn set_config(config: &mut SetConfigArgs) -> Result<()> {
    let mut komodefi_cfg = KomodefiConfigImpl::from_config_path().unwrap_or_else(|_| KomodefiConfigImpl::default());

    if let Some(path) = &config.from_path {
        let (uri, password) = config.source_from_path(path)?;
        komodefi_cfg.set_rpc_uri(uri);
        komodefi_cfg.set_rpc_password(password);
    } else {
        config.inquire()?;
        let set_password = config
            .password
            .ok_or_else(|| error_anyhow!("No set password option detected in config"))?;
        let rpc_api_uri = config.uri.take();
        assert!(set_password || rpc_api_uri.is_some());

        if set_password {
            let rpc_password = Password::new("Enter RPC API password:")
                .prompt()
                .map_err(|error| error_anyhow!("Failed to get rpc_api_password: {error}"))?;
            komodefi_cfg.set_rpc_password(rpc_password);
        }

        if let Some(rpc_api_uri) = rpc_api_uri {
            validate_rpc_url(&rpc_api_uri)?;
            komodefi_cfg.set_rpc_uri(rpc_api_uri);
        }
    }

    komodefi_cfg.write_to_config_path()?;
    info!("Configuration has been set");

    Ok(())
}

/// Validates an RPC URL and add base if not provided.
pub fn validate_rpc_url(input: &str) -> Result<(), anyhow::Error> {
    let url = match Url::parse(input) {
        Ok(url) => url,
        Err(err) => match err {
            ParseError::RelativeUrlWithoutBase => {
                Url::parse(&format!("http://{}", input)).map_err(|err| error_anyhow!("Invalid RPC URI: {err:?}"))?
            },
            _ => return Err(error_anyhow!("Invalid RPC URI: {err:?}")),
        },
    };

    // Check that the scheme is "http" and the host is an IPv4 address, and there is a port
    if url.scheme() == "http" && url.port().is_some() {
        Ok(())
    } else {
        Err(error_anyhow!(
            "Invalid RPC URI! Expected scheme, host, and port (http:127.0.0.1:7783)"
        ))
    }
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
            Self::warn_on_insecure_mode(komodefi_path_str)?;
        }
        Ok(())
    }

    #[cfg(unix)]
    fn warn_on_insecure_mode(file_path: &str) -> Result<()> {
        let perms = fs::metadata(file_path)?.permissions();
        let mode = perms.mode() & 0o777;
        if mode != CFG_FILE_PERM_MODE {
            warn!(
                "Configuration file: '{}' - does not comply to the expected mode: {:o}, the actual one is: {:o}",
                file_path, CFG_FILE_PERM_MODE, mode
            );

            // Update permissions to the desired mode
            info!("Updating permission...");
            let new_perms = fs::Permissions::from_mode(CFG_FILE_PERM_MODE);
            fs::set_permissions(Path::new(file_path), new_perms)?;
            info!("Permission Updated!, new permission: {mode}");
        };

        Ok(())
    }

    fn set_rpc_password(&mut self, rpc_password: String) { self.rpc_password.replace(rpc_password); }

    fn set_rpc_uri(&mut self, rpc_uri: String) { self.rpc_uri.replace(rpc_uri); }

    fn print_config(&self, unhide: bool) -> String {
        if !self.is_set() {
            return "komodefi configuration is not set".to_string();
        }
        let password = if unhide {
            self.rpc_password.clone().expect("Expected rpc_password is not set")
        } else {
            String::from("*************")
        };

        format!(
            "mm2 RPC URL: {}\nmm2 RPC password: {}",
            self.rpc_uri.as_deref().expect("Expected rpc_uri is not set"),
            password
        )
    }
}
