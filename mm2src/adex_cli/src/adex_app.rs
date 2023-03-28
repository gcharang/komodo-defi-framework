use std::env;
use std::io::Write;

use super::adex_config::AdexConfigImpl;
use super::adex_proc::ResponseHandlerImpl;
use super::cli;

pub(crate) struct AdexApp {
    config: AdexConfigImpl,
}

impl AdexApp {
    pub fn new() -> Result<AdexApp, ()> {
        let config = AdexConfigImpl::read_config()?;
        Ok(AdexApp { config })
    }

    pub async fn execute(&self) {
        let mut writer = std::io::stdout();
        let response_handler = ResponseHandlerImpl {
            writer: (&mut writer as &mut dyn Write).into(),
        };
        let _ = cli::Cli::execute(env::args(), &self.config, &response_handler).await;
    }
}
