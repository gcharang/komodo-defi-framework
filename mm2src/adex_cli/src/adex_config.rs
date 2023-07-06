use anyhow::{anyhow, bail, Result};
use directories::ProjectDirs;
use inquire::Password;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::env;
use std::env::var_os;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::fs;
use std::mem::take;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::adex_proc::SmartFractPrecision;
use crate::helpers::rewrite_json_file;
use crate::logging::{error_anyhow, warn_bail};

const PROJECT_QUALIFIER: &str = "com";
const PROJECT_COMPANY: &str = "komodoplatform";
const PROJECT_APP: &str = "adex-cli";
const ADEX_CFG: &str = "adex_cfg.json";
const RPC_PASSWORD_ENV: &str = "KOMODO_RPC_PASSWORD";

const PRICE_PRECISION_MIN: usize = 8;
const PRICE_PRECISION_MAX: usize = 8;
const VOLUME_PRECISION_MIN: usize = 2;
const VOLUME_PRECISION_MAX: usize = 5;
const VOLUME_PRECISION: SmartFractPrecision = (VOLUME_PRECISION_MIN, VOLUME_PRECISION_MAX);
const PRICE_PRECISION: SmartFractPrecision = (PRICE_PRECISION_MIN, PRICE_PRECISION_MAX);

pub(super) fn get_config() {
    let Ok(adex_cfg) = AdexConfigImpl::from_config_path() else { return; };
    info!("{}", adex_cfg)
}

pub(super) fn set_config(set_password: bool, rpc_api_uri: Option<String>) -> Result<()> {
    assert!(set_password || rpc_api_uri.is_some());
    let mut adex_cfg = AdexConfigImpl::from_config_path().unwrap_or_else(|_| AdexConfigImpl::default());

    if let Some(rpc_api_uri) = rpc_api_uri {
        adex_cfg.set_rpc_uri(rpc_api_uri);
    }

    adex_cfg.write_to_config_path()?;
    info!("Configuration has been set");

    Ok(())
}

fn inquire_password() -> Result<String> {
    Password::new("Enter RPC API password:")
        .prompt()
        .map_err(|error| error_anyhow!("Failed to get rpc_api_password: {}", error))
}

pub(super) trait AdexConfig {
    fn rpc_password(&self) -> Option<String>;
    fn rpc_uri(&self) -> Option<String>;
    fn orderbook_price_precision(&self) -> &SmartFractPrecision;
    fn orderbook_volume_precision(&self) -> &SmartFractPrecision;
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct AdexConfigImpl {
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rpc_uri: Option<String>,
}

impl AdexConfig for AdexConfigImpl {
    fn rpc_password(&self) -> Option<String> {
        match env::var(RPC_PASSWORD_ENV) {
            Ok(rpc_password) => Some(rpc_password),
            Err(_) => {
                let rpc_password = inquire_password()
                    .map_err(|error| error!("Failed to inquire rpc password: {}", error))
                    .ok()?;
                info!("asdfa");
                Self::set_rpc_password(&rpc_password).ok()?;
                Some(rpc_password)
            },
        }
    }
    fn rpc_uri(&self) -> Option<String> { self.rpc_uri.clone() }
    fn orderbook_price_precision(&self) -> &SmartFractPrecision { &PRICE_PRECISION }
    fn orderbook_volume_precision(&self) -> &SmartFractPrecision { &VOLUME_PRECISION }
}

impl Display for AdexConfigImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.is_set() {
            return writeln!(f, "adex configuration is not set");
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

impl AdexConfigImpl {
    #[cfg(test)]
    pub(super) fn new(rpc_password: &str, rpc_uri: &str) -> Self {
        Self {
            rpc_password: Some(rpc_password.to_string()),
            rpc_uri: Some(rpc_uri.to_string()),
        }
    }

    #[cfg(not(test))]
    pub(super) fn read_config() -> Result<AdexConfigImpl> {
        let config = AdexConfigImpl::from_config_path()?;
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

    fn get_config_path() -> Result<PathBuf> {
        let mut config_path = Self::get_config_dir()?;
        config_path.push(ADEX_CFG);
        Ok(config_path)
    }

    fn from_config_path() -> Result<AdexConfigImpl> {
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

    fn read_from(cfg_path: &Path) -> Result<AdexConfigImpl> {
        let adex_path_str = cfg_path.to_str().unwrap_or("Undefined");
        let adex_cfg_file = fs::File::open(cfg_path)
            .map_err(|error| error_anyhow!("Failed to open: {adex_path_str}, error: {error}"))?;

        serde_json::from_reader(adex_cfg_file)
            .map_err(|error| error_anyhow!("Failed to read adex_cfg to read from: {adex_path_str}, error: {error}"))
    }

    fn write_to(&self, cfg_path: &Path) -> Result<()> {
        let adex_path_str = cfg_path
            .to_str()
            .ok_or_else(|| error_anyhow!("Failed to get cfg_path as str"))?;
        rewrite_json_file(self, adex_path_str)
    }

    fn set_rpc_password(rpc_password: &String) -> Result<()> {
        env::set_var(RPC_PASSWORD_ENV, &rpc_password);
        let mut command = Shell::get_shell().get_setenv_command(RPC_PASSWORD_ENV, rpc_password.as_str());
        debug!("command: {:?}", command);
        command
            .status()
            .map_err(|error| error_anyhow!("Failed to store: {}, error: {}", RPC_PASSWORD_ENV, error))?;
        info!("Environment: {RPC_PASSWORD_ENV} has been set");
        Ok(())
    }

    fn set_rpc_uri(&mut self, rpc_uri: String) { self.rpc_uri.replace(rpc_uri); }
}

enum Shell {
    Windows,
    /// The default if we can't figure out the shell
    Bash,
    Tcsh,
    Zsh,
    Ksh,
}

impl Shell {
    fn get_shell() -> Shell {
        if cfg!(windows) {
            Shell::Windows
        } else {
            if let Some(shell) = var_os("BASH") {
                if shell.to_string_lossy().ends_with("/bash") {
                    return Shell::Bash;
                }
            }

            if let Some(zsh) = var_os("ZSH_NAME") {
                if zsh.to_string_lossy() == "zsh" {
                    return Shell::Zsh;
                }
            }

            if let Some(shell) = var_os("shell") {
                if shell.to_string_lossy().ends_with("/tcsh") {
                    return Shell::Tcsh;
                }
            }
            return match var_os("SHELL") {
                None => Shell::Bash,
                Some(oss) => {
                    if oss.to_string_lossy().ends_with("/bash") {
                        Shell::Bash
                    } else if oss.to_string_lossy().ends_with("/ksh") {
                        Shell::Ksh
                    } else if oss.to_string_lossy().ends_with("/zsh") {
                        Shell::Zsh
                    } else if oss.to_string_lossy().ends_with("/tcsh") {
                        Shell::Tcsh
                    } else {
                        Shell::Bash
                    }
                },
            };
        }
    }

    fn get_setenv_command<K: AsRef<OsStr>, V: AsRef<OsStr>>(&self, k: K, v: V) -> Command {
        match *self {
            Shell::Windows => {
                let mut command = Command::new("set");
                command.arg(format!(
                    "{}={}",
                    k.as_ref().to_string_lossy(),
                    v.as_ref().to_string_lossy()
                ));
                command
            },
            Shell::Tcsh => {
                let mut command = Command::new("setenv");
                command
                    .arg(format!("{}", k.as_ref().to_string_lossy()))
                    .arg(format!("'{}'", v.as_ref().to_string_lossy()));
                command
            },
            _ => {
                let mut command = Command::new("declare");
                std::os::unix::process::command.arg("-x").arg(format!(
                    "{}='{}'",
                    k.as_ref().to_string_lossy(),
                    v.as_ref().to_string_lossy()
                ));
                command
            },
        }
    }
}
