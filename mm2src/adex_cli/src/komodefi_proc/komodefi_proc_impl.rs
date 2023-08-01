use anyhow::{anyhow, bail, Result};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::time::Duration;
use uuid::Uuid;

use common::log::{debug, error, info, warn};
use mm2_rpc::data::legacy::{BalanceRequest, BalanceResponse, BanPubkeysRequest, BuyRequest, CancelAllOrdersRequest,
                            CancelAllOrdersResponse, CancelBy, CancelOrderRequest, CoinInitResponse,
                            GetEnabledResponse, MakerOrderForRpc, MinTradingVolResponse, Mm2RpcResult,
                            MmVersionResponse, MyOrdersRequest, MyOrdersResponse, OrderStatusRequest,
                            OrderStatusResponse, OrderbookDepthRequest, OrderbookRequest, OrderbookResponse,
                            OrdersHistoryRequest, OrdersHistoryResponse, PairWithDepth, SellBuyResponse, SellRequest,
                            SetPriceReq, SetRequiredConfRequest, SetRequiredNotaRequest, Status, StopRequest,
                            UpdateMakerOrderRequest, VersionRequest};
use mm2_rpc::data::version2::{BestOrdersRequestV2, GetRawTransactionRequest, MmRpcRequest, MmRpcResponseV2,
                              MmRpcResultV2, MmRpcVersion};

use self::macros::{request_legacy, request_v2};
use super::command::{Command, V2Method};
use super::response_handler::ResponseHandler;
use super::{OrderbookSettings, OrdersHistorySettings};
use crate::activation_scheme_db::get_activation_scheme;
use crate::komodefi_config::KomodefiConfig;
use crate::rpc_data::activation::{zcoin::ZcoinActivationParams, ActivationMethod, ActivationMethodV2,
                                  EnablePlatformCoinWithTokensReq, InitRpcTaskResponse, InitStandaloneCoinReq,
                                  RpcTaskStatusRequest, TaskId};
use crate::rpc_data::{bch, ActiveSwapsRequest, ActiveSwapsResponse, CancelRpcTaskError, CancelRpcTaskRequest,
                      CoinsToKickStartRequest, CoinsToKickstartResponse, DisableCoinRequest, DisableCoinResponse,
                      GetEnabledRequest, GetGossipMeshRequest, GetGossipMeshResponse, GetGossipPeerTopicsRequest,
                      GetGossipPeerTopicsResponse, GetGossipTopicPeersRequest, GetGossipTopicPeersResponse,
                      GetMyPeerIdRequest, GetMyPeerIdResponse, GetPeersInfoRequest, GetPeersInfoResponse,
                      GetRelayMeshRequest, GetRelayMeshResponse, ListBannedPubkeysRequest, ListBannedPubkeysResponse,
                      MaxTakerVolRequest, MaxTakerVolResponse, MinTradingVolRequest, MyRecentSwapResponse,
                      MyRecentSwapsRequest, MySwapStatusRequest, MySwapStatusResponse, Params,
                      RecoverFundsOfSwapRequest, RecoverFundsOfSwapResponse, SendRawTransactionRequest,
                      SendRawTransactionResponse, SetRequiredConfResponse, SetRequiredNotaResponse,
                      TradePreimageRequest, UnbanPubkeysRequest, UnbanPubkeysResponse, WithdrawRequest};
use crate::transport::Transport;
use crate::{error_anyhow, error_bail, warn_anyhow};

pub(crate) struct KomodefiProc<'trp, 'hand, 'cfg, T: Transport, H: ResponseHandler, C: KomodefiConfig + ?Sized> {
    pub(crate) transport: Option<&'trp T>,
    pub(crate) response_handler: &'hand H,
    pub(crate) config: &'cfg C,
}

impl<T: Transport, P: ResponseHandler, C: KomodefiConfig + 'static> KomodefiProc<'_, '_, '_, T, P, C> {
    pub(crate) async fn enable(&self, coin: &str, keep_progress: u64) -> Result<()> {
        info!("Enabling coin: {coin}");
        let activation_scheme = get_activation_scheme()?;
        let activation_method = activation_scheme.get_activation_method(coin)?;

        match activation_method {
            ActivationMethod::Legacy(method) => {
                let enable = Command::builder()
                    .flatten_data(method)
                    .userpass(self.get_rpc_password()?)
                    .build()?;

                request_legacy!(enable, CoinInitResponse, self, on_enable_response)
            },
            ActivationMethod::V2(ActivationMethodV2::EnableBchWithTokens(params)) => self.enable_bch(params).await,
            ActivationMethod::V2(ActivationMethodV2::EnableSlp(params)) => {
                let enable_slp = self.command_v2(V2Method::EnableSlp, params)?;
                request_v2!(self, enable_slp, on_enable_slp ; print_response).await
            },
            ActivationMethod::V2(ActivationMethodV2::EnableTendermintWithAssets(params)) => {
                let enable_tendermint = self.command_v2(V2Method::EnableTendermintWithAssets, params)?;
                request_v2!(
                    self,
                    enable_tendermint,
                    on_enable_tendermint ; print_response
                )
                .await
            },
            ActivationMethod::V2(ActivationMethodV2::EnableTendermintToken(params)) => {
                let enable_tendermint_token = self.command_v2(V2Method::EnableTendermintToken, params)?;
                request_v2!(
                    self,
                    enable_tendermint_token,
                    on_enable_tendermint_token ; print_response
                )
                .await
            },
            ActivationMethod::V2(ActivationMethodV2::EnableEthWithTokens(params)) => {
                let enable_erc20 = self.command_v2(V2Method::EnableEthWithTokens, params)?;
                request_v2!(
                    self,
                    enable_erc20,
                    on_enable_eth_with_tokens ; print_response
                )
                .await
            },
            ActivationMethod::V2(ActivationMethodV2::EnableErc20(params)) => {
                let enable_erc20 = self.command_v2(V2Method::EnableErc20, params)?;
                request_v2!(self, enable_erc20, on_enable_erc20 ; print_response).await
            },

            ActivationMethod::V2(ActivationMethodV2::EnableZCoin(params)) => {
                self.enable_z_coin(params, keep_progress).await
            },
        }
    }

    pub(crate) async fn disable(&self, request: DisableCoinRequest) -> Result<()> {
        info!("Disabling coin: {}", request.coin);
        let disable_command = self.command_legacy(request)?;
        request_legacy!(disable_command, DisableCoinResponse, self, on_disable_coin)
    }

    pub(crate) async fn get_balance(&self, request: BalanceRequest) -> Result<()> {
        info!("Getting balance, coin: {}", request.coin);
        let get_balance = self.command_legacy(request)?;
        request_legacy!(get_balance, BalanceResponse, self, on_balance_response)
    }

    pub(crate) async fn get_enabled(&self) -> Result<()> {
        info!("Getting list of enabled coins ...");
        let enabled = self.command_legacy(GetEnabledRequest::default())?;
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
        let sell = self.command_legacy(request)?;
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
        let buy = self.command_legacy(request)?;
        request_legacy!(buy, Mm2RpcResult<SellBuyResponse>, self, on_buy_response)
    }

    pub(crate) async fn send_stop(&self) -> Result<()> {
        info!("Sending stop command");
        let stop_command = self.command_legacy(StopRequest::default())?;
        request_legacy!(stop_command, Mm2RpcResult<Status>, self, on_stop_response)
    }

    pub(crate) async fn get_version(&self) -> Result<()> {
        info!("Requesting for mm2 version");
        let get_version = self.command_legacy(VersionRequest::default())?;
        request_legacy!(get_version, MmVersionResponse, self, on_version_response)
    }

    pub(crate) async fn cancel_order(&self, request: CancelOrderRequest) -> Result<()> {
        info!("Cancelling order: {}", request.uuid);
        let cancel_order = self.command_legacy(request)?;
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
        let cancel_all = self.command_legacy(request)?;
        request_legacy!(
            cancel_all,
            Mm2RpcResult<CancelAllOrdersResponse>,
            self,
            on_cancel_all_response
        )
    }

    pub(crate) async fn order_status(&self, request: OrderStatusRequest) -> Result<()> {
        info!("Getting order status: {}", request.uuid);
        let order_status = self.command_legacy(request)?;
        request_legacy!(order_status, OrderStatusResponse, self, on_order_status)
    }

    pub(crate) async fn my_orders(&self) -> Result<()> {
        info!("Getting my orders");
        let my_orders = self.command_legacy(MyOrdersRequest::default())?;
        request_legacy!(my_orders, Mm2RpcResult<MyOrdersResponse>, self, on_my_orders)
    }

    pub(crate) async fn best_orders(&self, params: BestOrdersRequestV2, show_orig_tickets: bool) -> Result<()> {
        info!("Getting best orders: {} {}", params.action, params.coin);
        let best_orders_command = self.command_v2(V2Method::BestOrders, params)?;
        request_v2!(
            self,
            best_orders_command,
            on_best_orders, show_orig_tickets;
            print_response
        )
        .await
    }

    pub(crate) async fn set_price(&self, request: SetPriceReq) -> Result<()> {
        info!("Setting price for pair: {} {}", request.base, request.rel);
        let set_price = self.command_legacy(request)?;
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
        let ob_depth = self.command_legacy(request)?;
        request_legacy!(ob_depth, Mm2RpcResult<Vec<PairWithDepth>>, self, on_orderbook_depth)
    }

    pub(crate) async fn orders_history(
        &self,
        request: OrdersHistoryRequest,
        settings: OrdersHistorySettings,
    ) -> Result<()> {
        info!("Getting order history");
        let get_history = self.command_legacy(request)?;
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
        let update_maker_order = self.command_legacy(request)?;
        request_legacy!(
            update_maker_order,
            Mm2RpcResult<MakerOrderForRpc>,
            self,
            on_update_maker_order
        )
    }

    pub(crate) async fn active_swaps(&self, include_status: bool, uuids_only: bool) -> Result<()> {
        info!("Getting active swaps");
        let active_swaps_command = self.command_legacy(ActiveSwapsRequest { include_status })?;
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
        let recent_swaps_command = self.command_legacy(request)?;
        request_legacy!(
            recent_swaps_command,
            Mm2RpcResult<MyRecentSwapResponse>,
            self,
            on_my_recent_swaps
        )
    }

    pub(crate) async fn min_trading_vol(&self, coin: String) -> Result<()> {
        info!("Getting min trading vol: {}", coin);
        let min_trading_vol_command = self.command_legacy(MinTradingVolRequest { coin })?;
        request_legacy!(
            min_trading_vol_command,
            Mm2RpcResult<MinTradingVolResponse>,
            self,
            on_min_trading_vol
        )
    }

    pub(crate) async fn max_taker_vol(&self, coin: String) -> Result<()> {
        info!("Getting max taker vol, {}", coin);
        let max_taker_vol_command = self.command_legacy(MaxTakerVolRequest { coin })?;
        request_legacy!(max_taker_vol_command, MaxTakerVolResponse, self, on_max_taker_vol)
    }

    pub(crate) async fn recover_funds_of_swap(&self, request: RecoverFundsOfSwapRequest) -> Result<()> {
        info!("Recovering funds of swap: {}", request.params.uuid);
        let recover_funds_command = self.command_legacy(request)?;
        request_legacy!(
            recover_funds_command,
            RecoverFundsOfSwapResponse,
            self,
            on_recover_funds
        )
    }

    pub(crate) async fn trade_preimage(&self, request: TradePreimageRequest) -> Result<()> {
        info!("Getting trade preimage");
        let trade_preimage_command = self.command_v2(V2Method::TradePreimage, request)?;
        request_v2!(self, trade_preimage_command, on_trade_preimage ; print_response).await
    }

    pub(crate) async fn get_gossip_mesh(&self) -> Result<()> {
        info!("Getting gossip mesh");
        let get_gossip_mesh_command = self.command_legacy(GetGossipMeshRequest::default())?;
        request_legacy!(
            get_gossip_mesh_command,
            Mm2RpcResult<GetGossipMeshResponse>,
            self,
            on_gossip_mesh
        )
    }

    pub(crate) async fn get_relay_mesh(&self) -> Result<()> {
        info!("Getting relay mesh");
        let get_relay_mesh_command = self.command_legacy(GetRelayMeshRequest::default())?;
        request_legacy!(
            get_relay_mesh_command,
            Mm2RpcResult<GetRelayMeshResponse>,
            self,
            on_relay_mesh
        )
    }

    pub(crate) async fn get_gossip_peer_topics(&self) -> Result<()> {
        info!("Getting gossip peer topics");
        let get_gossip_peer_topics_command = self.command_legacy(GetGossipPeerTopicsRequest::default())?;
        request_legacy!(
            get_gossip_peer_topics_command,
            Mm2RpcResult<GetGossipPeerTopicsResponse>,
            self,
            on_gossip_peer_topics
        )
    }

    pub(crate) async fn get_gossip_topic_peers(&self) -> Result<()> {
        info!("Getting gossip topic peers");
        let get_gossip_topic_peers = self.command_legacy(GetGossipTopicPeersRequest::default())?;
        request_legacy!(
            get_gossip_topic_peers,
            Mm2RpcResult<GetGossipTopicPeersResponse>,
            self,
            on_gossip_topic_peers
        )
    }

    pub(crate) async fn get_my_peer_id(&self) -> Result<()> {
        info!("Getting my peer id");
        let get_my_peer_id_command = self.command_legacy(GetMyPeerIdRequest::default())?;
        request_legacy!(
            get_my_peer_id_command,
            Mm2RpcResult<GetMyPeerIdResponse>,
            self,
            on_my_peer_id
        )
    }

    pub(crate) async fn get_peers_info(&self) -> Result<()> {
        info!("Getting peers info");
        let peers_info_command = self.command_legacy(GetPeersInfoRequest::default())?;
        request_legacy!(
            peers_info_command,
            Mm2RpcResult<GetPeersInfoResponse>,
            self,
            on_peers_info
        )
    }

    pub(crate) async fn set_required_confirmations(&self, request: SetRequiredConfRequest) -> Result<()> {
        info!(
            "Setting required confirmations: {}, confirmations: {}",
            request.coin, request.confirmations
        );
        let set_required_conf_command = self.command_legacy(request)?;
        request_legacy!(
            set_required_conf_command,
            Mm2RpcResult<SetRequiredConfResponse>,
            self,
            on_set_confirmations
        )
    }

    pub(crate) async fn set_required_nota(&self, request: SetRequiredNotaRequest) -> Result<()> {
        info!(
            "Setting required nota: {}, requires_nota: {}",
            request.coin, request.requires_notarization
        );
        let set_nota_command = self.command_legacy(request)?;
        request_legacy!(
            set_nota_command,
            Mm2RpcResult<SetRequiredNotaResponse>,
            self,
            on_set_notarization
        )
    }

    pub(crate) async fn coins_to_kick_start(&self) -> Result<()> {
        info!("Getting coins needed for kickstart");
        let coins_to_kick_start_command = self.command_legacy(CoinsToKickStartRequest::default())?;
        request_legacy!(
            coins_to_kick_start_command,
            Mm2RpcResult<CoinsToKickstartResponse>,
            self,
            on_coins_to_kickstart
        )
    }

    pub(crate) async fn ban_pubkey(&self, request: BanPubkeysRequest) -> Result<()> {
        info!("Banning pubkey: {}", request.pubkey);
        let ban_pubkey_command = self.command_legacy(request)?;
        request_legacy!(ban_pubkey_command, Mm2RpcResult<Status>, self, on_ban_pubkey)
    }

    pub(crate) async fn list_banned_pubkeys(&self) -> Result<()> {
        info!("Getting list of banned pubkeys");
        let list_banned_command = self.command_legacy(ListBannedPubkeysRequest::default())?;
        request_legacy!(
            list_banned_command,
            Mm2RpcResult<ListBannedPubkeysResponse>,
            self,
            on_list_banned_pubkeys
        )
    }

    pub(crate) async fn unban_pubkeys(&self, request: UnbanPubkeysRequest) -> Result<()> {
        info!("Unbanning pubkeys");
        let unban_pubkeys_command = self.command_legacy(request)?;
        request_legacy!(
            unban_pubkeys_command,
            Mm2RpcResult<UnbanPubkeysResponse>,
            self,
            on_unban_pubkeys
        )
    }

    pub(crate) async fn send_raw_transaction(
        &self,
        request: SendRawTransactionRequest,
        bare_output: bool,
    ) -> Result<()> {
        info!("Sending raw transaction");
        let send_raw_command = self.command_legacy(request)?;
        request_legacy!(
            send_raw_command,
            SendRawTransactionResponse,
            self,
            on_send_raw_transaction,
            bare_output
        )
    }

    pub(crate) async fn withdraw(&self, request: WithdrawRequest, bare_output: bool) -> Result<()> {
        info!("Getting withdraw tx_hex");
        debug!("Getting withdraw request: {:?}", request);
        let withdraw_command = self.command_v2(V2Method::Withdraw, request)?;
        request_v2!(self, withdraw_command, on_withdraw, bare_output ; print_response).await
    }

    pub(crate) async fn get_public_key(&self) -> Result<()> {
        info!("Getting public key");
        let pubkey_command = self.command_v2(V2Method::GetPublicKey, ())?;
        request_v2!(self, pubkey_command, on_public_key ; print_response).await
    }

    pub(crate) async fn get_public_key_hash(&self) -> Result<()> {
        info!("Getting public key hash");
        let pubkey_hash_command = self.command_v2(V2Method::GetPublicKeyHash, ())?;
        request_v2!(self, pubkey_hash_command, on_public_key_hash ; print_response).await
    }

    pub(crate) async fn get_raw_transaction(&self, request: GetRawTransactionRequest, bare_output: bool) -> Result<()> {
        info!(
            "Getting raw transaction of coin: {}, hash: {}",
            request.coin, request.tx_hash
        );
        let get_raw_tx_command = self.command_v2(V2Method::GetRawTransaction, request)?;
        request_v2!(
            self,
            get_raw_tx_command,
            on_raw_transaction, bare_output ;
            print_response
        )
        .await
    }

    async fn enable_bch(
        &self,
        params: EnablePlatformCoinWithTokensReq<bch::BchWithTokensActivationParams>,
    ) -> Result<()> {
        let enable_bch = self.command_v2(V2Method::EnableBchWithTokens, params)?;
        request_v2!(self, enable_bch, on_enable_bch ; print_response).await
    }

    async fn enable_z_coin(
        &self,
        params: InitStandaloneCoinReq<ZcoinActivationParams>,
        track_timeout_sec: u64,
    ) -> Result<()> {
        let enable_z_coin = self.command_v2(V2Method::EnableZCoin, params)?;

        let transport = self.transport.ok_or_else(|| {
            warn_anyhow!(concat!(
                "Failed to send: `",
                "enable_z_coin",
                "`, transport is not available"
            ))
        })?;
        match transport
            .send::<_, MmRpcResponseV2<InitRpcTaskResponse>, Json>(enable_z_coin)
            .await
        {
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Ok { result },
                id: _,
            })) => {
                let task_id = self.response_handler.on_enable_z_coin(result);
                if track_timeout_sec != 0 {
                    self.enable_zcoin_status(task_id, Some(track_timeout_sec)).await?;
                }
            },
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Err(error),
                id: _,
            })) => self.response_handler.on_mm_rpc_error_v2(error)?,
            Ok(Err(error)) => self.response_handler.print_response(error)?,
            Err(error) => error_bail!(
                concat!("Failed to send `", stringify!($request), "` request: {}"),
                error
            ),
        };
        Ok(())
    }

    pub(crate) async fn enable_zcoin_status(&self, task_id: TaskId, track_timeout_sec: Option<u64>) -> Result<()> {
        let zcoint_stat = self.command_v2(V2Method::EnableZCoinStatus, RpcTaskStatusRequest {
            task_id,
            forget_if_finished: true,
        })?;

        while request_v2!(self, zcoint_stat, on_zcoin_status ; print_response).await? {
            if let Some(track_timeout) = track_timeout_sec {
                tokio::time::sleep(Duration::from_secs(track_timeout)).await;
            } else {
                break;
            }
        }
        Ok(())
    }

    pub(crate) async fn enable_zcoin_cancel(&self, task_id: u64) -> Result<()> {
        let zcoin_cancel = self.command_v2(V2Method::EnableZCoinCancel, CancelRpcTaskRequest { task_id })?;
        request_v2!(
            zcoin_cancel,
            Status ; CancelRpcTaskError,
            self,
            on_enable_zcoin_cancel ; on_enable_zcoin_cancel_error
        )
    }

    fn command_legacy<R: Serialize>(&self, request: R) -> Result<Command<R>> {
        Command::builder()
            .userpass(self.get_rpc_password()?)
            .flatten_data(request)
            .build()
    }

    fn command_v2<R: Serialize>(&self, method: V2Method, params: R) -> Result<MmRpcRequest<V2Method, Option<R>>> {
        Command::builder()
            .userpass(self.get_rpc_password()?)
            .v2_method(method)
            .flatten_data(params)
            .build_v2()
    }

    fn get_rpc_password(&self) -> Result<String> {
        self.config
            .rpc_password()
            .ok_or_else(|| error_anyhow!("Failed to get rpc_password, not set"))
    }

    async fn request_v2<
        Req: Serialize + Send + Sync,
        Resp: for<'a> Deserialize<'a> + Send + Sync,
        RespErr: for<'a> Deserialize<'a> + Send + Sync,
        Res: Default,
        H: FnOnce(Resp) -> Result<Res>,
        EH: FnOnce(RespErr),
    >(
        &self,
        request: &Req,
        handle: H,
        err_handle: EH,
    ) -> Result<Res> {
        let transport = self
            .transport
            .ok_or_else(|| warn_anyhow!("Failed to send request, transport is not available"))?;
        match transport.send::<_, MmRpcResponseV2<Resp>, RespErr>(request).await {
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Ok { result },
                id: _,
            })) => handle(result),
            Ok(Ok(MmRpcResponseV2 {
                mmrpc: MmRpcVersion::V2,
                result: MmRpcResultV2::Err(error),
                id: _,
            })) => {
                self.response_handler.on_mm_rpc_error_v2(error)?;
                Ok(Res::default())
            },
            Ok(Err(error)) => {
                let _ = err_handle(error);
                Ok(Res::default())
            },
            Err(error) => error_bail!(
                concat!("Failed to send `", stringify!($request), "` request: {}"),
                error
            ),
        }
    }
}

mod macros {
    #[macro_export]
    macro_rules! request_legacy {
        ($request: ident, $response_ty: ty, $self: ident, $handle_method: ident$ (, $opt:expr)*) => {{
            let transport = $self.transport.ok_or_else(|| warn_anyhow!( concat!("Failed to send: `", stringify!($request), "`, transport is not available")))?;
            match transport.send::<_, $response_ty, Json>($request).await {
                Ok(Ok(ok)) => $self.response_handler.$handle_method(ok, $($opt),*),
                Ok(Err(error)) => $self.response_handler.print_response(error),
                Err(error) => error_bail!(concat!("Failed to send: `", stringify!($request), "`: {}"), error)
            }
        }};
    }
    #[macro_export]
    macro_rules! request_v2 {
        ($self: ident, $request: expr, $handle: ident$(, $opt:expr)* ;  $err_handle: ident) => {{
            $self
            .request_v2(
                &$request,
                |response| $self.response_handler.$handle(response, $($opt),*),
                |error| {
                    let _ = $self.response_handler.$err_handle(error);
                },
            )

        }};
        ($request: ident, $response_ty: ty ; $err_response_ty: ty, $self: ident, $handle_method: ident$(, $opt:expr)* ; $handle_err_method: ident) => {{
            let transport = $self.transport.ok_or_else(|| {
                warn_anyhow!(concat!(
                    "Failed to send: `",
                    stringify!($request),
                    "`, transport is not available"
                ))
            })?;
            match transport.send::<_, MmRpcResponseV2<$response_ty>, $err_response_ty>($request).await {
                Ok(Ok(MmRpcResponseV2 {
                    mmrpc: MmRpcVersion::V2,
                    result: MmRpcResultV2::Ok { result },
                    id: _,
                })) => $self.response_handler.$handle_method(result, $($opt),*),
                Ok(Ok(MmRpcResponseV2 {
                    mmrpc: MmRpcVersion::V2,
                    result: MmRpcResultV2::Err(error),
                    id: _,
                })) => $self.response_handler.on_mm_rpc_error_v2(error),
                Ok(Err(error)) => $self.response_handler.$handle_err_method(error),
                Err(error) => error_bail!(concat!("Failed to send `", stringify!($request), "` request: {}"), error),
            }
        }};

    }
    pub(super) use {request_legacy, request_v2};
}
