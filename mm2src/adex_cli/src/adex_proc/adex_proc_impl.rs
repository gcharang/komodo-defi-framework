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

macro_rules! request_legacy {
    ($request: ident, $response_ty: ty, $self: ident, $handle_method: ident$ (, $opt:expr)*) => {
        match $self.transport.send::<_, $response_ty, Json>($request).await {
            Ok(Ok(ok)) => $self.response_handler.$handle_method(ok, $($opt),*),
            Ok(Err(error)) => $self.response_handler.print_response(error),
            Err(error) => error_bail!(
                concat!("Failed to send ", stringify!($response_ty), " request: {}"),
                error
            ),
        }
    };
}

impl<T: Transport, P: ResponseHandler, C: AdexConfig + 'static> AdexProc<'_, '_, '_, T, P, C> {
    pub(crate) async fn enable(&self, asset: &str) -> Result<()> {
        info!("Enabling asset: {asset}");
        let activation_scheme = get_activation_scheme()?;
        let Some(activation_method) = activation_scheme.get_activation_method(asset) else {
            warn_bail!("Asset is not known: {asset}")
        };

        let command = Command::builder()
            .flatten_data(activation_method)
            .userpass(self.config.rpc_password()?)
            .build()?;

        request_legacy!(command, CoinInitResponse, self, on_enable_response)
    }

    pub(crate) async fn get_balance(&self, request: MyBalanceRequest) -> Result<()> {
        info!("Getting balance, coin: {}", request.coin);
        let command = Command::builder()
            .method(Method::GetBalance)
            .flatten_data(request)
            .userpass(self.config.rpc_password()?)
            .build()?;
        request_legacy!(command, MyBalanceResponse, self, on_balance_response)
    }

    pub(crate) async fn get_enabled(&self) -> Result<()> {
        info!("Getting list of enabled coins");
        let command = Command::<i32>::builder()
            .method(Method::GetEnabledCoins)
            .userpass(self.config.rpc_password()?)
            .build()?;

        request_legacy!(command, Mm2RpcResult<GetEnabledResponse>, self, on_get_enabled_response)
    }

    pub(crate) async fn get_orderbook(&self, request: OrderbookRequest, ob_settings: OrderbookSettings) -> Result<()> {
        info!("Getting orderbook, base: {}, rel: {}", request.base, request.rel);
        let command = Command::builder()
            .method(Method::GetOrderbook)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            command,
            OrderbookResponse,
            self,
            on_orderbook_response,
            self.config,
            ob_settings
        )
    }

    pub(crate) async fn sell(&self, request: SellBuyRequest) -> Result<()> {
        info!(
            "Selling: {} {} for: {} {} at the price of {} {} per {} ",
            request.volume,
            request.base,
            request.volume.clone() * request.price.clone(),
            request.rel,
            request.price,
            request.rel,
            request.base,
        );
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::Sell)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<SellBuyResponse>, self, on_sell_response)
    }

    pub(crate) async fn buy(&self, request: SellBuyRequest) -> Result<()> {
        info!(
            "Buying: {} {} with: {} {} at the price of {} {} per {} ",
            request.volume,
            request.base,
            request.volume.clone() * request.price.clone(),
            request.rel,
            request.price,
            request.rel,
            request.base,
        );
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::Buy)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<SellBuyResponse>, self, on_buy_response)
    }

    pub(crate) async fn send_stop(&self) -> Result<()> {
        info!("Sending stop command");
        let command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::Stop)
            .build()?;
        request_legacy!(command, Mm2RpcResult<Status>, self, on_stop_response)
    }

    pub(crate) async fn get_version(self) -> Result<()> {
        info!("Request for mm2 version");
        let command = Command::<Dummy>::builder().method(Method::Version).build()?;
        request_legacy!(command, MmVersionResponse, self, on_version_response)
    }

    pub(crate) async fn cancel_order(&self, order_id: &Uuid) -> Result<()> {
        info!("Cancelling order: {order_id} ");
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::CancelOrder)
            .flatten_data(CancelOrderRequest { uuid: *order_id })
            .build()?;
        request_legacy!(command, Mm2RpcResult<Status>, self, on_cancel_order_response)
    }

    pub(crate) async fn cancel_all_orders(&self) -> Result<()> {
        info!("Cancelling all orders");
        self.cancel_all_orders_impl(CancelBy::All).await
    }

    pub(crate) async fn cancel_by_pair(&self, base: String, rel: String) -> Result<()> {
        info!("Cancelling by pair, base: {base}, rel: {rel} ");
        self.cancel_all_orders_impl(CancelBy::Pair { base, rel }).await
    }

    pub(crate) async fn cancel_by_coin(&self, ticker: String) -> Result<()> {
        info!("Cancelling by coin: {ticker} ");
        self.cancel_all_orders_impl(CancelBy::Coin { ticker }).await
    }

    async fn cancel_all_orders_impl(&self, cancel_by: CancelBy) -> Result<()> {
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::CancelAllOrders)
            .flatten_data(CancelAllOrdersRequest { cancel_by })
            .build()?;
        request_legacy!(
            command,
            Mm2RpcResult<CancelAllOrdersResponse>,
            self,
            on_cancel_all_response
        )
    }

    pub(crate) async fn order_status(&self, uuid: &Uuid) -> Result<()> {
        info!("Getting order status: {uuid} ");
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::OrderStatus)
            .flatten_data(OrderStatusRequest { uuid: *uuid })
            .build()?;
        request_legacy!(command, OrderStatusResponse, self, on_order_status)
    }

    pub(crate) async fn my_orders(&self) -> Result<()> {
        info!("Getting my orders");
        let command = Command::<Dummy>::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::MyOrders)
            .build()?;
        request_legacy!(command, Mm2RpcResult<MyOrdersResponse>, self, on_my_orders)
    }

    pub(crate) async fn best_orders(&self, request: BestOrdersRequestV2, show_orig_tickets: bool) -> Result<()> {
        info!("Getting best orders: {} {} ", request.action, request.coin);
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::BestOrders)
            .flatten_data(request)
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
            Err(error) => error_bail!("Failed to send BestOrdersRequestV2 request: {error}"),
        }
    }

    pub(crate) async fn set_price(&self, request: SetPriceReq) -> Result<()> {
        info!("Setting price for pair: {} {} ", request.base, request.rel);
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::SetPrice)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<MakerOrderForRpc>, self, on_set_price)
    }

    pub(crate) async fn orderbook_depth(&self, request: OrderbookDepthRequest) -> Result<()> {
        info!(
            "Getting orderbook depth for pairs: {} ",
            request
                .pairs
                .iter()
                .map(|pair| format!("{}/{}", pair.0, pair.1))
                .join(", ")
        );
        let request = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::OrderbookDepth)
            .flatten_data(request)
            .build()?;
        request_legacy!(request, Mm2RpcResult<Vec<PairWithDepth>>, self, on_orderbook_depth)
    }

    pub(crate) async fn orders_history(
        &self,
        request: OrdersHistoryRequest,
        settings: OrdersHistorySettings,
    ) -> Result<()> {
        info!("Getting order history");
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::OrdersHistory)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            command,
            Mm2RpcResult<OrdersHistoryResponse>,
            self,
            on_orders_history,
            settings
        )
    }

    pub(crate) async fn update_maker_order(&self, request: UpdateMakerOrderRequest) -> Result<()> {
        info!("Updating maker order");
        let command = Command::builder()
            .userpass(self.config.rpc_password()?)
            .method(Method::UpdateMakerOrder)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<MakerOrderForRpc>, self, on_update_maker_order)
    }
}
