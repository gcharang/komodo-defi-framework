use anyhow::{bail, Result};
use itertools::Itertools;
use log::{error, info, warn};
use mm2_rpc::data::legacy::{CancelAllOrdersRequest, CancelAllOrdersResponse, CancelBy, CancelOrderRequest,
                            CoinInitResponse, GetEnabledResponse, MakerOrderForRpc, Mm2RpcResult, MmVersionResponse,
                            MyBalanceRequest, MyBalanceResponse, MyOrdersResponse, OrderStatusRequest,
                            OrderStatusResponse, OrderbookDepthRequest, OrderbookRequest, OrderbookResponse,
                            OrdersHistoryRequest, OrdersHistoryResponse, PairWithDepth, SellBuyRequest,
                            SellBuyResponse, SetPriceReq, Status, UpdateMakerOrderRequest};
use mm2_rpc::data::version2::{BestOrdersRequestV2, BestOrdersV2Response, MmRpcResponseV2, MmRpcResultV2};
use serde_json::Value as Json;
use uuid::Uuid;

use super::command::{Command, Dummy, Method};
use super::response_handler::ResponseHandler;
use super::{OrderbookSettings, OrdersHistorySettings};
use crate::activation_scheme_db::get_activation_scheme;
use crate::adex_config::AdexConfig;
use crate::transport::Transport;
use crate::{error_bail, warn_bail};

pub(crate) struct AdexProc<'trp, 'hand, 'cfg, T: Transport, H: ResponseHandler, C: AdexConfig + ?Sized> {
    pub(crate) transport: &'trp T,
    pub(crate) response_handler: &'hand H,
    pub(crate) config: &'cfg C,
}

impl<T: Transport, P: ResponseHandler, C: AdexConfig + 'static> AdexProc<'_, '_, '_, T, P, C> {
    pub(crate) async fn enable(&self, asset: &str) -> Result<()> {
        info!("Enabling asset: {asset} ...");

        let activation_scheme = get_activation_scheme()?;
        let Some(activation_method) = activation_scheme.get_activation_method(asset) else {
            warn_bail!("Asset is not known: {asset}")
        };

        let command = Command::builder()
            .flatten_data(activation_method)
            .userpass(self.config.rpc_password()?)
            .build()?;

        match self.transport.send::<_, CoinInitResponse, Json>(command).await {
            Ok(Ok(ref ok)) => self.response_handler.on_enable_response(ok),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send enable request, asset: {asset}, error: {error}"),
        }
    }

    pub(crate) async fn get_balance(&self, asset: &str) -> Result<()> {
        info!("Getting balance, coin: {asset} ...");
        let command = Command::builder()
            .method(Method::GetBalance)
            .flatten_data(MyBalanceRequest {
                coin: asset.to_string(),
            })
            .userpass(self.config.rpc_password()?)
            .build()?;

        match self.transport.send::<_, MyBalanceResponse, Json>(command).await {
            Ok(Ok(balance_response)) => self.response_handler.on_balance_response(&balance_response),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send get-balance request: {error}"),
        }
    }

    pub(crate) async fn get_enabled(&self) -> Result<()> {
        info!("Getting list of enabled coins ...");

        let command = Command::<i32>::builder()
            .method(Method::GetEnabledCoins)
            .userpass(self.config.rpc_password()?)
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<GetEnabledResponse>, Json>(command)
            .await
        {
            Ok(Ok(ok)) => self.response_handler.on_get_enabled_response(&ok),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send get-enabled-coins request: {error}"),
        }
    }

    pub(crate) async fn get_orderbook(&self, base: &str, rel: &str, ob_settings: OrderbookSettings) -> Result<()> {
        info!("Getting orderbook, base: {base}, rel: {rel} ...");

        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::GetOrderbook)
            .flatten_data(OrderbookRequest {
                base: base.to_string(),
                rel: rel.to_string(),
            })
            .build()?;

        match self.transport.send::<_, OrderbookResponse, Json>(command).await {
            Ok(Ok(ok)) => self
                .response_handler
                .on_orderbook_response(ok, self.config, ob_settings),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send get-orderbook request: {error}"),
        }
    }

    pub(crate) async fn sell(&self, order: SellBuyRequest) -> Result<()> {
        info!(
            "Selling: {} {} for: {} {} at the price of {} {} per {} ...",
            order.volume,
            order.base,
            order.volume.clone() * order.price.clone(),
            order.rel,
            order.price,
            order.rel,
            order.base,
        );

        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
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
            Err(error) => error_bail!("Failed to send sell request: {error}"),
        }
    }

    pub(crate) async fn buy(&self, order: SellBuyRequest) -> Result<()> {
        info!(
            "Buying: {} {} with: {} {} at the price of {} {} per {} ...",
            order.volume,
            order.base,
            order.volume.clone() * order.price.clone(),
            order.rel,
            order.price,
            order.rel,
            order.base,
        );

        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
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
            Err(error) => error_bail!("Failed to send buy request: {error}"),
        }
    }

    pub(crate) async fn send_stop(&self) -> Result<()> {
        info!("Sending stop command ...");
        let stop_command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::Stop)
            .build()?;

        match self.transport.send::<_, Mm2RpcResult<Status>, Json>(stop_command).await {
            Ok(Ok(ok)) => self.response_handler.on_stop_response(&ok),
            Ok(Err(error)) => error_bail!("Failed to stop through the API: {error}"),
            Err(error) => error_bail!("Failed to send stop request: {error}"),
        }
    }

    pub(crate) async fn get_version(self) -> Result<()> {
        info!("Request for mm2 version ...");
        let version_command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::Version)
            .build()?;

        match self.transport.send::<_, MmVersionResponse, Json>(version_command).await {
            Ok(Ok(ok)) => self.response_handler.on_version_response(&ok),
            Ok(Err(error)) => error_bail!("Failed get version through the API: {error}"),
            Err(error) => error_bail!("Failed to send get-version request: {error}"),
        }
    }

    pub async fn cancel_order(&self, order_id: &Uuid) -> Result<()> {
        info!("Cancelling order: {order_id} ...");
        let cancel_command = Command::builder()
            .userpass(self.config.rpc_password()?)
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
            Err(error) => error_bail!("Failed to send cancel-order request: {error}"),
        }
    }

    pub async fn cancel_all_orders(&self) -> Result<()> {
        info!("Cancelling all orders ...");
        self.cancel_all_orders_impl(CancelBy::All).await
    }

    pub async fn cancel_by_pair(&self, base: String, rel: String) -> Result<()> {
        info!("Cancelling by pair, base: {base}, rel: {rel} ...");
        self.cancel_all_orders_impl(CancelBy::Pair { base, rel }).await
    }

    pub async fn cancel_by_coin(&self, ticker: String) -> Result<()> {
        info!("Cancelling by coin: {ticker} ...");
        self.cancel_all_orders_impl(CancelBy::Coin { ticker }).await
    }

    async fn cancel_all_orders_impl(&self, cancel_by: CancelBy) -> Result<()> {
        let cancel_all_orders = Command::builder()
            .userpass(self.config.rpc_password()?)
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
            Err(error) => error_bail!("Failed to send candel-all-orders request: {error}"),
        }
    }

    pub async fn order_status(&self, uuid: &Uuid) -> Result<()> {
        info!("Getting order status: {uuid} ...");
        let order_status_command = Command::builder()
            .userpass(self.config.rpc_password()?)
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
            Err(error) => error_bail!("Failed to send order-status request: {error}"),
        }
    }

    pub async fn my_orders(&self) -> Result<()> {
        info!("Getting my orders ...");
        let command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::MyOrders)
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<MyOrdersResponse>, Json>(command)
            .await
        {
            Ok(Ok(Mm2RpcResult { result })) => self.response_handler.on_my_orders(result),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send my-orders request: {error}"),
        }
    }

    pub async fn best_orders(&self, best_orders_request: BestOrdersRequestV2, show_orig_tickets: bool) -> Result<()> {
        info!(
            "Getting best orders: {} {} ...",
            best_orders_request.action, best_orders_request.coin
        );
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::BestOrders)
            .flatten_data(best_orders_request)
            .build_v2()?;

        match self
            .transport
            .send::<_, MmRpcResponseV2<BestOrdersV2Response>, Json>(command)
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
                error_bail!("Got error: {:?}", error)
            },
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send best-orders request: {error}"),
        }
    }

    pub async fn set_price(&self, set_price_request: SetPriceReq) -> Result<()> {
        info!(
            "Setting price for pair: {} {} ...",
            set_price_request.base, set_price_request.rel
        );
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::SetPrice)
            .flatten_data(set_price_request)
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<MakerOrderForRpc>, Json>(command)
            .await
        {
            Ok(Ok(Mm2RpcResult { result })) => self.response_handler.on_set_price(result),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send set-price request: {error}"),
        }
    }

    pub async fn orderbook_depth(&self, orderbook_depth_request: OrderbookDepthRequest) -> Result<()> {
        info!(
            "Getting orderbook depth for pairs: {} ...",
            orderbook_depth_request
                .pairs
                .iter()
                .map(|pair| format!("{}/{}", pair.0, pair.1))
                .join(", ")
        );

        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::OrderbookDepth)
            .flatten_data(orderbook_depth_request)
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<Vec<PairWithDepth>>, Json>(command)
            .await
        {
            Ok(Ok(Mm2RpcResult { result })) => self.response_handler.on_orderbook_depth(result),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send orderbook-depth request: {error}"),
        }
    }

    pub async fn orders_history(
        &self,
        orders_history_request: OrdersHistoryRequest,
        settings: OrdersHistorySettings,
    ) -> Result<()> {
        info!("Getting order history ...");
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::OrdersHistory)
            .flatten_data(orders_history_request)
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<OrdersHistoryResponse>, Json>(command)
            .await
        {
            Ok(Ok(Mm2RpcResult { result })) => self.response_handler.on_orders_history(result, settings),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send history request: {error}"),
        }
    }

    pub async fn update_maker_order(&self, update_maker_request: UpdateMakerOrderRequest) -> Result<()> {
        info!("Updating maker order ...");
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::UpdateMakerOrder)
            .flatten_data(update_maker_request)
            .build()?;

        match self
            .transport
            .send::<_, Mm2RpcResult<MakerOrderForRpc>, Json>(command)
            .await
        {
            Ok(Ok(Mm2RpcResult { result })) => self.response_handler.on_update_maker_order(result),
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send update-maker-order request: {error}"),
        }
    }
}
