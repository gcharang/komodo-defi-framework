use crate::error_anyhow;
use anyhow::{anyhow, Result};
use clap::Args;
use common::log::error;
use inquire::{Confirm, Text};

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
}

impl SetConfigArgs {
    pub fn inquire(&mut self) -> Result<()> {
        self.inquire_password()?;
        self.inquire_uri()?;

        Ok(())
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
}
