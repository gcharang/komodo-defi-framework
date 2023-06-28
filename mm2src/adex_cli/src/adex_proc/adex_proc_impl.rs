use anyhow::{anyhow, bail, Result};
use itertools::Itertools;
use log::{error, info, warn};
use mm2_rpc::data::legacy::{BuyRequest, CancelAllOrdersRequest, CancelAllOrdersResponse, CancelBy, CancelOrderRequest,
                            CoinInitResponse, GetEnabledRequest, GetEnabledResponse, MakerOrderForRpc, Mm2RpcResult,
                            MmVersionResponse, MyBalanceRequest, MyBalanceResponse, MyOrdersRequest, MyOrdersResponse,
                            OrderStatusRequest, OrderStatusResponse, OrderbookDepthRequest, OrderbookRequest,
                            OrderbookResponse, OrdersHistoryRequest, OrdersHistoryResponse, PairWithDepth,
                            SellBuyRequest, SellBuyResponse, SellRequest, SetPriceReq, Status, StopRequest,
                            UpdateMakerOrderRequest, VersionRequest};
use mm2_rpc::data::version2::{BestOrdersRequestV2, BestOrdersV2Response, MmRpcResponseV2, MmRpcResultV2, MmRpcVersion};
use serde_json::Value as Json;

use super::command::{Command, V2Method};
use super::response_handler::ResponseHandler;
use super::{OrderbookSettings, OrdersHistorySettings};
use crate::activation_scheme_db::get_activation_scheme;
use crate::adex_config::AdexConfig;
use crate::transport::Transport;
use crate::{error_anyhow, error_bail, warn_anyhow, warn_bail};

pub(crate) struct AdexProc<'trp, 'hand, 'cfg, T: Transport, H: ResponseHandler, C: AdexConfig + ?Sized> {
    pub(crate) transport: Option<&'trp T>,
    pub(crate) response_handler: &'hand H,
    pub(crate) config: &'cfg C,
}

macro_rules! request_legacy {
    ($request: ident, $response_ty: ty, $self: ident, $handle_method: ident$ (, $opt:expr)*) => {{
        let transport = $self.transport.ok_or_else(|| warn_anyhow!("Failed to send, transport is not available"))?;
        match transport.send::<_, $response_ty, Json>($request).await {
            Ok(Ok(ok)) => $self.response_handler.$handle_method(ok, $($opt),*),
            Ok(Err(error)) => $self.response_handler.print_response(error),
            Err(error) => error_bail!(
                concat!("Failed to send ", stringify!($response_ty), " request: {}"),
                error
            ),
        }
    }};
}

impl<T: Transport, P: ResponseHandler, C: AdexConfig + 'static> AdexProc<'_, '_, '_, T, P, C> {
    pub(crate) async fn enable(&self, coin: &str) -> Result<()> {
        info!("Enabling coin: {coin}");
        let activation_scheme = get_activation_scheme()?;
        let Some(activation_method) = activation_scheme.get_activation_method(coin) else {
            warn_bail!("Coin is not known: {coin}")
        };

        let command = Command::builder()
            .flatten_data(activation_method)
            .userpass(self.get_rpc_password()?)
            .build()?;

        request_legacy!(command, CoinInitResponse, self, on_enable_response)
    }

    pub(crate) async fn get_balance(&self, request: MyBalanceRequest) -> Result<()> {
        info!("Getting balance, coin: {}", request.coin);
        let command = Command::builder()
            .flatten_data(request)
            .userpass(self.get_rpc_password()?)
            .build()?;
        request_legacy!(command, MyBalanceResponse, self, on_balance_response)
    }

    pub(crate) async fn get_enabled(&self) -> Result<()> {
        info!("Getting list of enabled coins");
        let command = Command::builder()
            .flatten_data(GetEnabledRequest::default())
            .userpass(self.get_rpc_password()?)
            .build()?;

        request_legacy!(command, Mm2RpcResult<GetEnabledResponse>, self, on_get_enabled_response)
    }

    pub(crate) async fn get_orderbook(&self, request: OrderbookRequest, ob_settings: OrderbookSettings) -> Result<()> {
        info!("Getting orderbook, base: {}, rel: {}", request.base, request.rel);
        let command = Command::builder().flatten_data(request).build()?;
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
            "Selling: {} {} for: {} {} at the price of {} {} per {}",
            request.volume,
            request.base,
            request.volume.clone() * request.price.clone(),
            request.rel,
            request.price,
            request.rel,
            request.base,
        );
        let request = SellRequest { delegate: request };
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<SellBuyResponse>, self, on_sell_response)
    }

    pub(crate) async fn buy(&self, request: SellBuyRequest) -> Result<()> {
        info!(
            "Buying: {} {} with: {} {} at the price of {} {} per {}",
            request.volume,
            request.base,
            request.volume.clone() * request.price.clone(),
            request.rel,
            request.price,
            request.rel,
            request.base,
        );
        let request = BuyRequest { delegate: request };
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<SellBuyResponse>, self, on_buy_response)
    }

    pub(crate) async fn send_stop(&self) -> Result<()> {
        info!("Sending stop command");
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(StopRequest::default())
            .build()?;
        request_legacy!(command, Mm2RpcResult<Status>, self, on_stop_response)
    }

    pub(crate) async fn get_version(&self) -> Result<()> {
        info!("Request for mm2 version");
        let command = Command::builder().flatten_data(VersionRequest::default()).build()?;
        request_legacy!(command, MmVersionResponse, self, on_version_response)
    }

    pub(crate) async fn cancel_order(&self, request: CancelOrderRequest) -> Result<()> {
        info!("Cancelling order: {}", request.uuid);
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<Status>, self, on_cancel_order_response)
    }

    pub(crate) async fn cancel_all_orders(&self) -> Result<()> {
        info!("Cancelling all orders");
        self.cancel_all_orders_impl(CancelAllOrdersRequest {
            cancel_by: CancelBy::All,
        })
        .await
    }

    pub(crate) async fn cancel_by_pair(&self, request: CancelAllOrdersRequest) -> Result<()> {
        let CancelBy::Pair { base, rel } = &request.cancel_by else {panic!("Bad cast to CancelBy::Pair")};
        info!("Cancelling by pair, base: {base}, rel: {rel}");
        self.cancel_all_orders_impl(request).await
    }

    pub(crate) async fn cancel_by_coin(&self, request: CancelAllOrdersRequest) -> Result<()> {
        let CancelBy::Coin { ticker } = &request.cancel_by else {panic!("Bad cast to CancelBy::Coin")};
        info!("Cancelling by coin: {ticker}");
        self.cancel_all_orders_impl(request).await
    }

    async fn cancel_all_orders_impl(&self, request: CancelAllOrdersRequest) -> Result<()> {
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            command,
            Mm2RpcResult<CancelAllOrdersResponse>,
            self,
            on_cancel_all_response
        )
    }

    pub(crate) async fn order_status(&self, request: OrderStatusRequest) -> Result<()> {
        info!("Getting order status: {}", request.uuid);
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, OrderStatusResponse, self, on_order_status)
    }

    pub(crate) async fn my_orders(&self) -> Result<()> {
        info!("Getting my orders");
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(MyOrdersRequest::default())
            .build()?;
        request_legacy!(command, Mm2RpcResult<MyOrdersResponse>, self, on_my_orders)
    }

    pub(crate) async fn best_orders(&self, request: BestOrdersRequestV2, show_orig_tickets: bool) -> Result<()> {
        info!("Getting best orders: {} {}", request.action, request.coin);
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .v2_method(V2Method::BestOrders)
            .flatten_data(request)
            .build_v2()?;
        let transport = self
            .transport
            .ok_or_else(|| warn_anyhow!("Failed to send, transport is not available"))?;

        match transport
            .send::<_, MmRpcResponseV2<BestOrdersV2Response>, Json>(command)
            .await
        {
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Ok { result },
                id: _,
            })) => self.response_handler.on_best_orders(result, show_orig_tickets),
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
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
        info!("Setting price for pair: {} {}", request.base, request.rel);
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<MakerOrderForRpc>, self, on_set_price)
    }

    pub(crate) async fn orderbook_depth(&self, request: OrderbookDepthRequest) -> Result<()> {
        info!(
            "Getting orderbook depth for pairs: {}",
            request
                .pairs
                .iter()
                .map(|pair| format!("{}/{}", pair.0, pair.1))
                .join(", ")
        );
        let request = Command::builder()
            .userpass(self.get_rpc_password()?)
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
            .userpass(self.get_rpc_password()?)
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
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(command, Mm2RpcResult<MakerOrderForRpc>, self, on_update_maker_order)
    }

    fn get_rpc_password(&self) -> Result<String> {
        self.config
            .rpc_password()
            .ok_or_else(|| error_anyhow!("Failed to get rpc_password, not set"))
    }
}
