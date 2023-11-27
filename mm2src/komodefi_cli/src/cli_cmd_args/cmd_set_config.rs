use crate::error_anyhow;
use anyhow::{anyhow, Result};
use clap::Args;
use common::log::error;
use inquire::{Confirm, Text};
use std::fs::File;
use std::io::Read;

#[derive(Args)]
#[group(required = false, multiple = true)]
pub(crate) struct SetConfigArgs {
    #[arg(long, short, help = "Set if you are going to set up a password")]
    pub(crate) password: Option<bool>,
    #[arg(
        long,
        short,
        visible_alias = "url",
        help = "KomoDeFi RPC API Uri. http://localhost:7783"
    )]
    pub(crate) uri: Option<String>,
    #[arg(long, short, help = "Set configuration from path")]
    pub(crate) from_path: Option<String>,
}

impl SetConfigArgs {
    pub fn inquire(&mut self) -> Result<()> {
        self.inquire_password()?;
        self.inquire_uri()
    }

    pub fn inquire_password(&mut self) -> Result<()> {
        if self.password.is_none() {
            self.password = Confirm::new("Set if you are going to set up a password:")
                .with_default(false)
                .with_placeholder("No")
                .prompt()
                .map_err(|error| error_anyhow!("Failed to get password option: {error}"))?
                .into();
        }

        Ok(())
    }

    pub fn inquire_uri(&mut self) -> Result<()> {
        if self.uri.is_none() {
            self.uri = Text::new("What is the rpc_uri:")
                .with_default("127.0.0.1:7783")
                .with_placeholder("127.0.0.1:7783")
                .prompt()
                .map_err(|error| error_anyhow!("Failed to get rpc_uri: {error}"))?
                .into();
        }
        Ok(())
    }

    pub fn source_from_path(&self, path: &str) -> Result<(String, String)> {
        let mut file = File::open(path).map_err(|error| error_anyhow!("Failed to get rpc_uri: {error}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|error| error_anyhow!("Failed to get rpc_uri: {error}"))?;

        let mm2_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&buffer))
            .map_err(|error| error_anyhow!("Failed to get rpc_uri: {error}"))?;
        let rpcip = &mm2_json["rpcip"];
        let rpcport = &mm2_json["rpcport"];
        let rpc_password = &mm2_json["rpc_password"];

        Ok((format!("{rpcip}:{rpcport}"), rpc_password.to_string()))
    }
}
