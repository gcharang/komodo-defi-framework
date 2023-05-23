use log::{error, info, warn};

use mm2_rpc_data::legacy::{BalanceResponse, CancelAllOrdersRequest, CancelAllOrdersResponse, CancelBy,
                           CancelOrderRequest, CoinInitResponse, GetEnabledResponse, Mm2RpcResult, MmVersionResponse,
                           OrderStatusRequest, OrderStatusResponse, OrderbookRequest, OrderbookResponse,
                           SellBuyRequest, SellBuyResponse, Status};
use mm2_rpc_data::version2::{BestOrdersRequestV2, BestOrdersV2Response, MmRpcResponseV2, MmRpcResultV2};
use serde_json::{json, Value as Json};
use uuid::Uuid;

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
            .build()?;

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
            .build()?;

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
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<GetEnabledResponse>, Json>(command)
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
            .build()?;

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
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<SellBuyResponse>, Json>(command)
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
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<SellBuyResponse>, Json>(command)
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
            .build()?;

        match self.transport.send::<_, Mm2RpcResult<Status>, Json>(stop_command).await {
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
            .build()?;

        match self.transport.send::<_, MmVersionResponse, Json>(version_command).await {
            Ok(Ok(ok)) => self.response_handler.on_version_response(&ok),
            Ok(Err(error)) => {
                error!("Failed get version through the API: {error}");
                Err(())
            },
            _ => Err(()),
        }
    }

    pub async fn cancel_order(&self, order_id: &Uuid) -> Result<(), ()> {
        info!("Cancelling order: {order_id}");
        let cancel_command = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::CancelOrder)
            .flatten_data(CancelOrderRequest { uuid: *order_id })
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<Status>, Json>(cancel_command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_cancel_order_response(&ok),
            Ok(Err(error)) => self.response_handler.print_response(error),
            _ => Err(()),
        }
    }

    pub async fn cancel_all_orders(&self) -> Result<(), ()> {
        info!("Cancelling all orders");
        self.cancel_all_orders_impl(CancelBy::All).await
    }

    pub async fn cancel_by_pair(&self, base: String, rel: String) -> Result<(), ()> {
        info!("Cancelling by pair, base: {base}, rel: {rel}");
        self.cancel_all_orders_impl(CancelBy::Pair { base, rel }).await
    }

    pub async fn cancel_by_coin(&self, ticker: String) -> Result<(), ()> {
        info!("Cancelling by coin: {ticker}");
        self.cancel_all_orders_impl(CancelBy::Coin { ticker }).await
    }

    async fn cancel_all_orders_impl(&self, cancel_by: CancelBy) -> Result<(), ()> {
        let cancel_all_orders = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::CancelAllOrders)
            .flatten_data(CancelAllOrdersRequest { cancel_by })
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<CancelAllOrdersResponse>, Json>(cancel_all_orders)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_cancel_all_response(&ok),
            Ok(Err(error)) => self.response_handler.print_response(error),
            _ => Err(()),
        }
    }

    pub async fn order_status(&self, uuid: &Uuid) -> Result<(), ()> {
        info!("Getting order status: {uuid}");
        let order_status_command = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::OrderStatus)
            .flatten_data(OrderStatusRequest { uuid: *uuid })
            .build()?;
        match self
            .transport
            .send::<_, OrderStatusResponse, Json>(order_status_command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_order_status(&ok),
            Ok(Err(error)) => self.response_handler.print_response(error),
            _ => Err(()),
        }
    }

    pub async fn best_orders(
        &self,
        best_orders_request: BestOrdersRequestV2,
        show_orig_tickets: bool,
    ) -> Result<(), ()> {
        info!(
            "Getting best orders: {} {}",
            best_orders_request.action, best_orders_request.coin
        );
        let best_orders_command = Command::builder()
            .userpass(self.config.rpc_password())
            .method(Method::BestOrders)
            .flatten_data(best_orders_request)
            .build_v2()?;

        match self
            .transport
            .send::<_, MmRpcResponseV2<BestOrdersV2Response>, Json>(best_orders_command)
            .await
        {
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: _,
                result: MmRpcResultV2::Ok { result },
                id: _,
            })) => self.response_handler.on_best_orders(result, show_orig_tickets),
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: _,
                result: MmRpcResultV2::Err(error),
                id: _,
            })) => {
                error!("Got error: {:?}", error);
                Err(())
            },
            Ok(Err(error)) => self.response_handler.print_response(error),
            _ => Err(()),
        }
    }
}
