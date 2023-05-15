use log::{error, info, warn};
use mm2_rpc_data::legacy::{BalanceResponse, CoinInitResponse, GetEnabledResponse, KmdWalletRpcResult,
                           MmVersionResponse, OrderbookRequest, OrderbookResponse, SellBuyRequest, SellBuyResponse,
                           Status};
use serde_json::{json, Value as Json};

use super::command::{Command, Dummy, Method};
use super::response_handler::ResponseHandler;
use super::OrderbookConfig;
use crate::activation_scheme_db::get_activation_scheme;
use crate::adex_config::AdexConfig;
use crate::transport::Transport;

pub(crate) struct AdexProc<'trp, 'hand, 'cfg, T: Transport, H: ResponseHandler, C: AdexConfig + ?Sized> {
    pub transport: &'trp T,
    pub response_handler: &'hand H,
    pub config: &'cfg C,
}

impl<T: Transport, P: ResponseHandler, C: AdexConfig + 'static> AdexProc<'_, '_, '_, T, P, C> {
    pub async fn enable(&self, asset: &str) -> Result<(), ()> {
        info!("Enabling asset: {asset}");

        let activation_scheme = get_activation_scheme();
        let Some(activate_specific_settings) = activation_scheme.get_activation_method(asset) else {
            warn!("Asset is not known: {asset}");
            return Err(());
        };

        let command = Command::builder()
            .flatten_data(activate_specific_settings.clone())
            .userpass(self.config.rpc_password())
            .build();

        match self.transport.send::<_, CoinInitResponse, Json>(command).await {
            Ok(Ok(ref ok)) => self.response_handler.on_enable_response(ok),
            Ok(Err(err)) => self.response_handler.print_response(err),
            Err(err) => {
                error!("Failed to enable asset: {asset}, error: {err:?}");
                Err(())
            },
        }
    }

    pub async fn get_balance(&self, asset: &str) -> Result<(), ()> {
        info!("Getting balance, coin: {asset} ...");
        let command = Command::builder()
            .method(Method::GetBalance)
            .flatten_data(json!({ "coin": asset }))
            .userpass(self.config.rpc_password())
            .build();

        match self.transport.send::<_, BalanceResponse, Json>(command).await {
            Ok(Ok(balance_response)) => self.response_handler.on_balance_response(&balance_response),
            Ok(Err(err)) => self.response_handler.print_response(err),
            Err(err) => {
                error!("Failed to get balance: {err:?}");
                Err(())
            },
        }
    }

    pub async fn get_enabled(&self) -> Result<(), ()> {
        info!("Getting list of enabled coins ...");

        let command = Command::<i32>::builder()
            .method(Method::GetEnabledCoins)
            .userpass(self.config.rpc_password())
            .build();

        match self
            .transport
            .send::<_, KmdWalletRpcResult<GetEnabledResponse>, Json>(command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_get_enabled_response(&ok),
            Ok(Err(err)) => self.response_handler.print_response(err),
            Err(err) => {
                error!("Failed to get enabled coins: {:?}", err);
                Err(())
            },
        }
    }

    pub async fn get_orderbook(&self, base: &str, rel: &str, orderbook_config: OrderbookConfig) -> Result<(), ()> {
        info!("Getting orderbook, base: {base}, rel: {rel} ...");

        let command = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::GetOrderbook)
            .flatten_data(OrderbookRequest {
                base: base.into(),
                rel: rel.into(),
            })
            .build();

        match self.transport.send::<_, OrderbookResponse, Json>(command).await {
            Ok(Ok(ok)) => self
                .response_handler
                .on_orderbook_response(ok, self.config, orderbook_config),
            Ok(Err(err)) => self.response_handler.print_response(err),
            Err(err) => {
                error!("Failed to get orderbook: {err:?}");
                Err(())
            },
        }
    }

    pub async fn sell(&self, order: SellBuyRequest) -> Result<(), ()> {
        info!(
            "Selling: {} {} for: {} {} at the price of {} {} per {}",
            order.volume,
            order.base,
            order.volume.clone() * order.price.clone(),
            order.rel,
            order.price,
            order.rel,
            order.base,
        );

        let command = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::Sell)
            .flatten_data(order)
            .build();

        match self
            .transport
            .send::<_, KmdWalletRpcResult<SellBuyResponse>, Json>(command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_sell_response(&ok),
            Ok(Err(err)) => self.response_handler.print_response(err),
            Err(err) => {
                error!("Failed to sell: {err:?}");
                Err(())
            },
        }
    }

    pub async fn buy(&self, order: SellBuyRequest) -> Result<(), ()> {
        info!(
            "Buying: {} {} for: {} {} at the price of {} {} per {}",
            order.volume,
            order.base,
            order.volume.clone() * order.price.clone(),
            order.rel,
            order.price,
            order.rel,
            order.base,
        );

        let command = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::Buy)
            .flatten_data(order)
            .build();

        match self
            .transport
            .send::<_, KmdWalletRpcResult<SellBuyResponse>, Json>(command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_buy_response(&ok),
            Ok(Err(err)) => self.response_handler.print_response(err),
            Err(err) => {
                error!("Failed to buy: {err:?}");
                Err(())
            },
        }
    }

    pub async fn send_stop(&self) -> Result<(), ()> {
        info!("Sending stop command");
        let stop_command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password())
            .method(Method::Stop)
            .build();

        match self
            .transport
            .send::<_, KmdWalletRpcResult<Status>, Json>(stop_command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_stop_response(&ok),
            Ok(Err(error)) => {
                error!("Failed to stop through the API: {error}");
                Err(())
            },
            _ => Err(()),
        }
    }

    pub async fn get_version(self) -> Result<(), ()> {
        info!("Request for mm2 version");
        let version_command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password())
            .method(Method::Version)
            .build();

        match self.transport.send::<_, MmVersionResponse, Json>(version_command).await {
            Ok(Ok(ok)) => self.response_handler.on_version_response(&ok),
            Ok(Err(error)) => {
                error!("Failed get version through the API: {error}");
                Err(())
            },
            _ => Err(()),
        }
    }
}
