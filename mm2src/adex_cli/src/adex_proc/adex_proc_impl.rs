use anyhow::{anyhow, bail, Result};
use itertools::Itertools;
use serde_json::Value as Json;

use common::log::{error, info, warn};
use mm2_rpc::data::legacy::{BalanceRequest, BalanceResponse, BuyRequest, CancelAllOrdersRequest,
                            CancelAllOrdersResponse, CancelBy, CancelOrderRequest, CoinInitResponse,
                            GetEnabledResponse, MakerOrderForRpc, MinTradingVolResponse, Mm2RpcResult,
                            MmVersionResponse, MyOrdersRequest, MyOrdersResponse, OrderStatusRequest,
                            OrderStatusResponse, OrderbookDepthRequest, OrderbookRequest, OrderbookResponse,
                            OrdersHistoryRequest, OrdersHistoryResponse, PairWithDepth, SellBuyResponse, SellRequest,
                            SetPriceReq, Status, StopRequest, UpdateMakerOrderRequest, VersionRequest};
use mm2_rpc::data::version2::{BestOrdersRequestV2, BestOrdersV2Response, MmRpcResponseV2, MmRpcResultV2, MmRpcVersion};
use uuid::Uuid;

use super::command::{Command, V2Method};
use super::response_handler::ResponseHandler;
use super::{OrderbookSettings, OrdersHistorySettings};
use crate::activation_scheme_db::get_activation_scheme;
use crate::adex_config::AdexConfig;
use crate::rpc_data::{ActiveSwapsRequest, ActiveSwapsResponse, GetEnabledRequest, GetGossipMeshRequest,
                      GetGossipMeshResponse, GetGossipPeerTopicsRequest, GetGossipPeerTopicsResponse,
                      GetGossipTopicPeersRequest, GetGossipTopicPeersResponse, GetMyPeerIdRequest,
                      GetMyPeerIdResponse, GetPeersInfoRequest, GetPeersInfoResponse, GetRelayMeshRequest,
                      GetRelayMeshResponse, MaxTakerVolRequest, MaxTakerVolResponse, MinTradingVolRequest,
                      MyRecentSwapResponse, MyRecentSwapsRequest, MySwapStatusRequest, MySwapStatusResponse, Params,
                      RecoverFundsOfSwapRequest, RecoverFundsOfSwapResponse, TradePreimageRequest,
                      TradePreimageResponse};
use crate::transport::Transport;
use crate::{error_anyhow, error_bail, warn_anyhow};

pub(crate) struct AdexProc<'trp, 'hand, 'cfg, T: Transport, H: ResponseHandler, C: AdexConfig + ?Sized> {
    pub(crate) transport: Option<&'trp T>,
    pub(crate) response_handler: &'hand H,
    pub(crate) config: &'cfg C,
}

macro_rules! request_legacy {
    ($request: ident, $response_ty: ty, $self: ident, $handle_method: ident$ (, $opt:expr)*) => {{
        let transport = $self.transport.ok_or_else(|| warn_anyhow!( concat!("Failed to send: `", stringify!($request), "`, transport is not available")))?;
        match transport.send::<_, $response_ty, Json>($request).await {
            Ok(Ok(ok)) => $self.response_handler.$handle_method(ok, $($opt),*),
            Ok(Err(error)) => $self.response_handler.print_response(error),
            Err(_) => error_bail!(concat!("Failed to send: `", stringify!($request), "`"))
        }
    }};
}

impl<T: Transport, P: ResponseHandler, C: AdexConfig + 'static> AdexProc<'_, '_, '_, T, P, C> {
    pub(crate) async fn enable(&self, coin: &str) -> Result<()> {
        info!("Enabling coin: {coin}");
        let activation_scheme = get_activation_scheme()?;
        let activation_method = activation_scheme.get_activation_method(coin)?;

        let enable = Command::builder()
            .flatten_data(activation_method)
            .userpass(self.get_rpc_password()?)
            .build()?;

        request_legacy!(enable, CoinInitResponse, self, on_enable_response)
    }

    pub(crate) async fn get_balance(&self, request: BalanceRequest) -> Result<()> {
        info!("Getting balance, coin: {}", request.coin);
        let get_balance = Command::builder()
            .flatten_data(request)
            .userpass(self.get_rpc_password()?)
            .build()?;
        request_legacy!(get_balance, BalanceResponse, self, on_balance_response)
    }

    pub(crate) async fn get_enabled(&self) -> Result<()> {
        info!("Getting list of enabled coins ...");

        let enabled = Command::builder()
            .flatten_data(GetEnabledRequest::default())
            .userpass(self.get_rpc_password()?)
            .build()?;
        request_legacy!(enabled, Mm2RpcResult<GetEnabledResponse>, self, on_get_enabled_response)
    }

    pub(crate) async fn get_orderbook(&self, request: OrderbookRequest, settings: OrderbookSettings) -> Result<()> {
        info!("Getting orderbook, base: {}, rel: {}", request.base, request.rel);

        let get_orderbook = Command::builder().flatten_data(request).build()?;
        request_legacy!(
            get_orderbook,
            OrderbookResponse,
            self,
            on_orderbook_response,
            self.config,
            settings
        )
    }

    pub(crate) async fn sell(&self, request: SellRequest) -> Result<()> {
        info!(
            "Selling: {} {} for: {} {} at the price of {} {} per {}",
            request.delegate.volume,
            request.delegate.base,
            request.delegate.volume.clone() * request.delegate.price.clone(),
            request.delegate.rel,
            request.delegate.price,
            request.delegate.rel,
            request.delegate.base,
        );
        let sell = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(sell, Mm2RpcResult<SellBuyResponse>, self, on_sell_response)
    }

    pub(crate) async fn buy(&self, request: BuyRequest) -> Result<()> {
        info!(
            "Buying: {} {} with: {} {} at the price of {} {} per {}",
            request.delegate.volume,
            request.delegate.base,
            request.delegate.volume.clone() * request.delegate.price.clone(),
            request.delegate.rel,
            request.delegate.price,
            request.delegate.rel,
            request.delegate.base,
        );
        let buy = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(buy, Mm2RpcResult<SellBuyResponse>, self, on_buy_response)
    }

    pub(crate) async fn send_stop(&self) -> Result<()> {
        info!("Sending stop command");
        let stop_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(StopRequest::default())
            .build()?;
        request_legacy!(stop_command, Mm2RpcResult<Status>, self, on_stop_response)
    }

    pub(crate) async fn get_version(&self) -> Result<()> {
        info!("Requesting for mm2 version");
        let get_version = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(VersionRequest::default())
            .build()?;
        request_legacy!(get_version, MmVersionResponse, self, on_version_response)
    }

    pub(crate) async fn cancel_order(&self, request: CancelOrderRequest) -> Result<()> {
        info!("Cancelling order: {}", request.uuid);
        let cancel_order = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(cancel_order, Mm2RpcResult<Status>, self, on_cancel_order_response)
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
        let cancel_all = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            cancel_all,
            Mm2RpcResult<CancelAllOrdersResponse>,
            self,
            on_cancel_all_response
        )
    }

    pub(crate) async fn order_status(&self, request: OrderStatusRequest) -> Result<()> {
        info!("Getting order status: {}", request.uuid);
        let order_status = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(order_status, OrderStatusResponse, self, on_order_status)
    }

    pub(crate) async fn my_orders(&self) -> Result<()> {
        info!("Getting my orders");
        let my_orders = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(MyOrdersRequest::default())
            .build()?;
        request_legacy!(my_orders, Mm2RpcResult<MyOrdersResponse>, self, on_my_orders)
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
        let set_price = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(set_price, Mm2RpcResult<MakerOrderForRpc>, self, on_set_price)
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
        let ob_depth = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(ob_depth, Mm2RpcResult<Vec<PairWithDepth>>, self, on_orderbook_depth)
    }

    pub(crate) async fn orders_history(
        &self,
        request: OrdersHistoryRequest,
        settings: OrdersHistorySettings,
    ) -> Result<()> {
        info!("Getting order history");
        let get_history = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            get_history,
            Mm2RpcResult<OrdersHistoryResponse>,
            self,
            on_orders_history,
            settings
        )
    }

    pub(crate) async fn update_maker_order(&self, request: UpdateMakerOrderRequest) -> Result<()> {
        info!("Updating maker order");
        let update_maker_order = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            update_maker_order,
            Mm2RpcResult<MakerOrderForRpc>,
            self,
            on_update_maker_order
        )
    }

    fn get_rpc_password(&self) -> Result<String> {
        self.config
            .rpc_password()
            .ok_or_else(|| error_anyhow!("Failed to get rpc_password, not set"))
    }

    pub(crate) async fn active_swaps(&self, include_status: bool, uuids_only: bool) -> Result<()> {
        info!("Getting active swaps");

        let active_swaps_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(ActiveSwapsRequest { include_status })
            .build()?;

        request_legacy!(
            active_swaps_command,
            ActiveSwapsResponse,
            self,
            on_active_swaps,
            uuids_only
        )
    }

    pub(crate) async fn swap_status(&self, uuid: Uuid) -> Result<()> {
        info!("Getting swap status: {}", uuid);
        let my_swap_status_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(MySwapStatusRequest {
                params: Params { uuid },
            })
            .build()?;
        request_legacy!(
            my_swap_status_command,
            Mm2RpcResult<MySwapStatusResponse>,
            self,
            on_my_swap_status
        )
    }

    pub(crate) async fn recent_swaps(&self, request: MyRecentSwapsRequest) -> Result<()> {
        info!("Getting recent swaps");
        let recent_swaps_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            recent_swaps_command,
            Mm2RpcResult<MyRecentSwapResponse>,
            self,
            on_my_recent_swaps
        )
    }

    pub(crate) async fn min_trading_vol(&self, coin: String) -> Result<()> {
        info!("Getting min trading vol: {}", coin);
        let min_trading_vol_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(MinTradingVolRequest { coin })
            .build()?;
        request_legacy!(
            min_trading_vol_command,
            Mm2RpcResult<MinTradingVolResponse>,
            self,
            on_min_trading_vol
        )
    }

    pub(crate) async fn max_taker_vol(&self, coin: String) -> Result<()> {
        info!("Getting max taker vol, {}", coin);
        let max_taker_vol_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(MaxTakerVolRequest { coin })
            .build()?;

        request_legacy!(max_taker_vol_command, MaxTakerVolResponse, self, on_max_taker_vol)
    }

    pub(crate) async fn recover_funds_of_swap(&self, request: RecoverFundsOfSwapRequest) -> Result<()> {
        info!("Recovering funds of swap: {}", request.params.uuid);
        let recover_funds_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()?;
        request_legacy!(
            recover_funds_command,
            RecoverFundsOfSwapResponse,
            self,
            on_recover_funds
        )
    }

    pub(crate) async fn trade_preimage(&self, request: TradePreimageRequest) -> Result<()> {
        info!("Getting trade preimage");
        let command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .v2_method(V2Method::TradePreimage)
            .flatten_data(request)
            .build_v2()?;

        let transport = self
            .transport
            .ok_or_else(|| warn_anyhow!("Failed to send `trade_preimage`, transport is not available"))?;

        match transport
            .send::<_, MmRpcResponseV2<TradePreimageResponse>, Json>(command)
            .await
        {
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Ok { result },
                id: _,
            })) => self.response_handler.on_trade_preimage(result),
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Err(error),
                id: _,
            })) => {
                error_bail!("Got error: {:?}", error)
            },
            Ok(Err(error)) => self.response_handler.print_response(error),
            Err(error) => error_bail!("Failed to send `trade_preimage` request: {error}"),
        }
    }

    pub(crate) async fn get_gossip_mesh(&self) -> Result<()> {
        info!("Getting gossip mesh");
        let get_gossip_mesh_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(GetGossipMeshRequest::default())
            .build()?;

        request_legacy!(
            get_gossip_mesh_command,
            Mm2RpcResult<GetGossipMeshResponse>,
            self,
            on_gossip_mesh
        )
    }

    pub(crate) async fn get_relay_mesh(&self) -> Result<()> {
        info!("Getting relay mesh");
        let get_relay_mesh_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(GetRelayMeshRequest::default())
            .build()?;
        request_legacy!(
            get_relay_mesh_command,
            Mm2RpcResult<GetRelayMeshResponse>,
            self,
            on_relay_mesh
        )
    }

    pub(crate) async fn get_gossip_peer_topics(&self) -> Result<()> {
        info!("Getting gossip peer topics");
        let get_gossip_peer_topics_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(GetGossipPeerTopicsRequest::default())
            .build()?;
        request_legacy!(
            get_gossip_peer_topics_command,
            Mm2RpcResult<GetGossipPeerTopicsResponse>,
            self,
            on_gossip_peer_topics
        )
    }

    pub(crate) async fn get_gossip_topic_peers(&self) -> Result<()> {
        info!("Getting gossip topic peers");
        let get_gossip_topic_peers = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(GetGossipTopicPeersRequest::default())
            .build()?;

        request_legacy!(
            get_gossip_topic_peers,
            Mm2RpcResult<GetGossipTopicPeersResponse>,
            self,
            on_gossip_topic_peers
        )
    }

    pub(crate) async fn get_my_peer_id(&self) -> Result<()> {
        info!("Getting my peer id");
        let get_my_peer_id_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(GetMyPeerIdRequest::default())
            .build()?;

        request_legacy!(
            get_my_peer_id_command,
            Mm2RpcResult<GetMyPeerIdResponse>,
            self,
            on_my_peer_id
        )
    }

    pub(crate) async fn get_peers_info(&self) -> Result<()> {
        info!("Getting peers info");
        let peers_info_command = Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(GetPeersInfoRequest::default())
            .build()?;
        request_legacy!(
            peers_info_command,
            Mm2RpcResult<GetPeersInfoResponse>,
            self,
            on_peers_info
        )
    }
}
