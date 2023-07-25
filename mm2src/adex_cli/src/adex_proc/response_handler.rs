#[path = "response_handler/activation.rs"] mod activation;
#[path = "response_handler/best_orders.rs"] mod best_orders;
#[path = "response_handler/formatters.rs"] mod formatters;
#[path = "response_handler/macros.rs"] mod macros;
#[path = "response_handler/my_orders.rs"] mod my_orders;
#[path = "response_handler/network.rs"] mod network;
#[path = "response_handler/order_status.rs"] mod order_status;
#[path = "response_handler/orderbook.rs"] mod orderbook;
#[path = "response_handler/orderbook_depth.rs"]
mod orderbook_depth;
#[path = "response_handler/orders_history.rs"]
mod orders_history;
#[path = "response_handler/smart_fraction_fmt.rs"]
mod smart_fraction_fmt;
#[path = "response_handler/swaps.rs"] mod swaps;
#[path = "response_handler/trading.rs"] mod trading;
#[path = "response_handler/utility.rs"] mod utility;
#[path = "response_handler/wallet.rs"] mod wallet;

pub(crate) use orderbook::OrderbookSettings;
pub(crate) use orders_history::OrdersHistorySettings;
pub(crate) use smart_fraction_fmt::SmartFractPrecision;

use anyhow::{anyhow, Result};
use itertools::Itertools;
use serde_json::Value as Json;
use std::cell::RefCell;
use std::io::Write;
use std::ops::DerefMut;

use common::log::error;
use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};
use mm2_rpc::data::legacy::{BalanceResponse, CancelAllOrdersResponse, CoinInitResponse, GetEnabledResponse,
                            MakerOrderForRpc, MinTradingVolResponse, Mm2RpcResult, MmVersionResponse,
                            MyOrdersResponse, OrderStatusResponse, OrderbookResponse, OrdersHistoryResponse,
                            PairWithDepth, SellBuyResponse, Status};
use mm2_rpc::data::version2::BestOrdersV2Response;

use crate::adex_config::AdexConfig;
use crate::adex_proc::response_handler::formatters::{writeln_field, ZERO_INDENT};
use crate::logging::error_anyhow;
use crate::rpc_data::{ActiveSwapsResponse, CoinsToKickstartResponse, DisableCoinResponse, GetGossipMeshResponse,
                      GetGossipPeerTopicsResponse, GetGossipTopicPeersResponse, GetMyPeerIdResponse,
                      GetPeersInfoResponse, GetRelayMeshResponse, ListBannedPubkeysResponse, MaxTakerVolResponse,
                      MyRecentSwapResponse, MySwapStatusResponse, RecoverFundsOfSwapResponse,
                      SendRawTransactionResponse, SetRequiredConfResponse, SetRequiredNotaResponse,
                      TradePreimageResponse, UnbanPubkeysResponse, WithdrawResponse};

pub(crate) trait ResponseHandler {
    fn print_response(&self, response: Json) -> Result<()>;

    fn on_orderbook_response<Cfg: AdexConfig + 'static>(
        &self,
        response: OrderbookResponse,
        config: &Cfg,
        settings: OrderbookSettings,
    ) -> Result<()>;
    fn on_get_enabled_response(&self, response: Mm2RpcResult<GetEnabledResponse>) -> Result<()>;
    fn on_version_response(&self, response: MmVersionResponse) -> Result<()>;
    fn on_enable_response(&self, response: CoinInitResponse) -> Result<()>;
    fn on_disable_coin(&self, response: DisableCoinResponse) -> Result<()>;
    fn on_balance_response(&self, response: BalanceResponse) -> Result<()>;
    fn on_sell_response(&self, response: Mm2RpcResult<SellBuyResponse>) -> Result<()>;
    fn on_buy_response(&self, response: Mm2RpcResult<SellBuyResponse>) -> Result<()>;
    fn on_stop_response(&self, response: Mm2RpcResult<Status>) -> Result<()>;
    fn on_cancel_order_response(&self, response: Mm2RpcResult<Status>) -> Result<()>;
    fn on_cancel_all_response(&self, response: Mm2RpcResult<CancelAllOrdersResponse>) -> Result<()>;
    fn on_order_status(&self, response: OrderStatusResponse) -> Result<()>;
    fn on_best_orders(&self, response: BestOrdersV2Response, show_orig_tickets: bool) -> Result<()>;
    fn on_my_orders(&self, response: Mm2RpcResult<MyOrdersResponse>) -> Result<()>;
    fn on_set_price(&self, response: Mm2RpcResult<MakerOrderForRpc>) -> Result<()>;
    fn on_orderbook_depth(&self, response: Mm2RpcResult<Vec<PairWithDepth>>) -> Result<()>;
    fn on_orders_history(
        &self,
        response: Mm2RpcResult<OrdersHistoryResponse>,
        settings: OrdersHistorySettings,
    ) -> Result<()>;
    fn on_update_maker_order(&self, response: Mm2RpcResult<MakerOrderForRpc>) -> Result<()>;
    fn on_active_swaps(&self, response: ActiveSwapsResponse, uuids_only: bool) -> Result<()>;
    fn on_my_swap_status(&self, response: Mm2RpcResult<MySwapStatusResponse>) -> Result<()>;
    fn on_my_recent_swaps(&self, response: Mm2RpcResult<MyRecentSwapResponse>) -> Result<()>;
    fn on_min_trading_vol(&self, response: Mm2RpcResult<MinTradingVolResponse>) -> Result<()>;
    fn on_max_taker_vol(&self, response: MaxTakerVolResponse) -> Result<()>;
    fn on_recover_funds(&self, response: RecoverFundsOfSwapResponse) -> Result<()>;
    fn on_trade_preimage(&self, response: TradePreimageResponse) -> Result<()>;
    fn on_gossip_mesh(&self, response: Mm2RpcResult<GetGossipMeshResponse>) -> Result<()>;
    fn on_relay_mesh(&self, response: Mm2RpcResult<GetRelayMeshResponse>) -> Result<()>;
    fn on_gossip_peer_topics(&self, response: Mm2RpcResult<GetGossipPeerTopicsResponse>) -> Result<()>;
    fn on_gossip_topic_peers(&self, response: Mm2RpcResult<GetGossipTopicPeersResponse>) -> Result<()>;
    fn on_my_peer_id(&self, response: Mm2RpcResult<GetMyPeerIdResponse>) -> Result<()>;
    fn on_peers_info(&self, response: Mm2RpcResult<GetPeersInfoResponse>) -> Result<()>;
    fn on_set_confirmations(&self, resonse: Mm2RpcResult<SetRequiredConfResponse>) -> Result<()>;
    fn on_set_notarization(&self, response: Mm2RpcResult<SetRequiredNotaResponse>) -> Result<()>;
    fn on_coins_to_kickstart(&self, response: Mm2RpcResult<CoinsToKickstartResponse>) -> Result<()>;
    fn on_ban_pubkey(&self, response: Mm2RpcResult<Status>) -> Result<()>;
    fn on_list_banned_pubkeys(&self, response: Mm2RpcResult<ListBannedPubkeysResponse>) -> Result<()>;
    fn on_unban_pubkeys(&self, response: Mm2RpcResult<UnbanPubkeysResponse>) -> Result<()>;
    fn on_send_raw_transaction(&self, response: SendRawTransactionResponse, raw_output: bool) -> Result<()>;
    fn on_withdraw(&self, response: WithdrawResponse, raw_output: bool) -> Result<()>;
}

pub(crate) struct ResponseHandlerImpl<'a> {
    pub(crate) writer: RefCell<&'a mut dyn Write>,
}

impl ResponseHandler for ResponseHandlerImpl<'_> {
    fn print_response(&self, result: Json) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        let object = result
            .as_object()
            .ok_or_else(|| error_anyhow!("Failed to cast result as object"))?;

        object
            .iter()
            .for_each(|value| writeln_safe_io!(writer, "{}: {:?}", value.0, value.1));
        Ok(())
    }

    fn on_orderbook_response<Cfg: AdexConfig + 'static>(
        &self,
        response: OrderbookResponse,
        config: &Cfg,
        settings: OrderbookSettings,
    ) -> Result<()> {
        orderbook::on_orderbook_response(self.writer.borrow_mut().deref_mut(), response, config, settings)
    }

    fn on_get_enabled_response(&self, response: Mm2RpcResult<GetEnabledResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        writeln_safe_io!(writer, "{:8} {}", "Ticker", "Address");
        for row in &response.result {
            writeln_safe_io!(writer, "{:8} {}", row.ticker, row.address);
        }
        Ok(())
    }

    fn on_version_response(&self, response: MmVersionResponse) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        writeln_safe_io!(writer, "Version: {}", response.result);
        writeln_safe_io!(writer, "Datetime: {}", response.datetime);
        Ok(())
    }

    fn on_enable_response(&self, response: CoinInitResponse) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        writeln_safe_io!(
            writer,
            "coin: {}\naddress: {}\nbalance: {}\nunspendable_balance: {}\nrequired_confirmations: {}\nrequires_notarization: {}",
            response.coin,
            response.address,
            response.balance,
            response.unspendable_balance,
            response.required_confirmations,
            if response.requires_notarization { "Yes" } else { "No" }
        );
        if let Some(mature_confirmations) = response.mature_confirmations {
            writeln_safe_io!(writer, "mature_confirmations: {}", mature_confirmations);
        }
        Ok(())
    }

    fn on_disable_coin(&self, response: DisableCoinResponse) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        activation::on_disable_coin(writer.deref_mut(), response);
        Ok(())
    }

    fn on_balance_response(&self, response: BalanceResponse) -> Result<()> {
        writeln_safe_io!(
            self.writer.borrow_mut(),
            "coin: {}\nbalance: {}\nunspendable: {}\naddress: {}",
            response.coin,
            response.balance,
            response.unspendable_balance,
            response.address
        );
        Ok(())
    }

    fn on_sell_response(&self, response: Mm2RpcResult<SellBuyResponse>) -> Result<()> {
        writeln_safe_io!(self.writer.borrow_mut(), "{}", response.request.uuid);
        Ok(())
    }

    fn on_buy_response(&self, response: Mm2RpcResult<SellBuyResponse>) -> Result<()> {
        writeln_safe_io!(self.writer.borrow_mut(), "{}", response.request.uuid);
        Ok(())
    }

    fn on_stop_response(&self, response: Mm2RpcResult<Status>) -> Result<()> {
        writeln_safe_io!(self.writer.borrow_mut(), "Service stopped: {}", response.result);
        Ok(())
    }

    fn on_cancel_order_response(&self, response: Mm2RpcResult<Status>) -> Result<()> {
        writeln_safe_io!(self.writer.borrow_mut(), "Order cancelled: {}", response.result);
        Ok(())
    }

    fn on_cancel_all_response(&self, response: Mm2RpcResult<CancelAllOrdersResponse>) -> Result<()> {
        let cancelled = &response.result.cancelled;
        let mut writer = self.writer.borrow_mut();
        if cancelled.is_empty() {
            writeln_safe_io!(writer, "No orders found to be cancelled");
        } else {
            writeln_safe_io!(writer, "Cancelled: {}", cancelled.iter().join(", "));
        }

        let currently_matched = &response.result.currently_matching;
        if !currently_matched.is_empty() {
            writeln_safe_io!(writer, "Currently matched: {}", currently_matched.iter().join(", "));
        }
        Ok(())
    }

    fn on_order_status(&self, response: OrderStatusResponse) -> Result<()> {
        order_status::on_order_status(self.writer.borrow_mut().deref_mut(), response)
    }

    fn on_best_orders(&self, response: BestOrdersV2Response, show_orig_tickets: bool) -> Result<()> {
        best_orders::on_best_orders(self.writer.borrow_mut().deref_mut(), response, show_orig_tickets)
    }

    fn on_my_orders(&self, response: Mm2RpcResult<MyOrdersResponse>) -> Result<()> {
        my_orders::on_my_orders(self.writer.borrow_mut().deref_mut(), response)
    }

    fn on_set_price(&self, response: Mm2RpcResult<MakerOrderForRpc>) -> Result<()> {
        formatters::on_maker_order_response(self.writer.borrow_mut().deref_mut(), response.result)
    }

    fn on_orderbook_depth(&self, response: Mm2RpcResult<Vec<PairWithDepth>>) -> Result<()> {
        orderbook_depth::on_orderbook_depth(self.writer.borrow_mut().deref_mut(), response)
    }

    fn on_orders_history(
        &self,
        response: Mm2RpcResult<OrdersHistoryResponse>,
        settings: OrdersHistorySettings,
    ) -> Result<()> {
        orders_history::on_orders_history(self.writer.borrow_mut().deref_mut(), response, settings)
    }

    fn on_update_maker_order(&self, response: Mm2RpcResult<MakerOrderForRpc>) -> Result<()> {
        formatters::on_maker_order_response(self.writer.borrow_mut().deref_mut(), response.result)
    }

    fn on_active_swaps(&self, response: ActiveSwapsResponse, uuids_only: bool) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        swaps::on_active_swaps(writer.deref_mut(), response, uuids_only)
    }

    fn on_my_swap_status(&self, response: Mm2RpcResult<MySwapStatusResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        swaps::on_my_swap_status(writer.deref_mut(), response.result)
    }

    fn on_my_recent_swaps(&self, response: Mm2RpcResult<MyRecentSwapResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        swaps::on_my_recent_swaps(writer.deref_mut(), response.result)
    }

    fn on_min_trading_vol(&self, response: Mm2RpcResult<MinTradingVolResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        trading::on_min_trading_vol(writer.deref_mut(), response.result)
    }

    fn on_max_taker_vol(&self, response: MaxTakerVolResponse) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        trading::on_max_taker_vol(writer.deref_mut(), response)
    }

    fn on_recover_funds(&self, response: RecoverFundsOfSwapResponse) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        swaps::on_recover_funds(writer.deref_mut(), response)
    }

    fn on_trade_preimage(&self, response: TradePreimageResponse) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        trading::on_trade_preimage(writer.deref_mut(), response)
    }

    fn on_gossip_mesh(&self, response: Mm2RpcResult<GetGossipMeshResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        network::on_gossip_mesh(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_relay_mesh(&self, response: Mm2RpcResult<GetRelayMeshResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        network::on_relay_mesh(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_gossip_peer_topics(&self, response: Mm2RpcResult<GetGossipPeerTopicsResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        network::on_gossip_peer_topics(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_gossip_topic_peers(&self, response: Mm2RpcResult<GetGossipTopicPeersResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        network::on_gossip_topic_peers(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_my_peer_id(&self, response: Mm2RpcResult<GetMyPeerIdResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        network::on_my_peer_id(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_peers_info(&self, response: Mm2RpcResult<GetPeersInfoResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        network::on_peers_info(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_set_confirmations(&self, response: Mm2RpcResult<SetRequiredConfResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        activation::on_set_confirmations(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_set_notarization(&self, response: Mm2RpcResult<SetRequiredNotaResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        activation::on_set_notarization(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_coins_to_kickstart(&self, response: Mm2RpcResult<CoinsToKickstartResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        activation::on_coins_to_kickstart(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_ban_pubkey(&self, response: Mm2RpcResult<Status>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        writeln_field(writer.deref_mut(), "Status", response.result, ZERO_INDENT);
        Ok(())
    }

    fn on_list_banned_pubkeys(&self, response: Mm2RpcResult<ListBannedPubkeysResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        utility::on_list_banned_pubkeys(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_unban_pubkeys(&self, response: Mm2RpcResult<UnbanPubkeysResponse>) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        utility::on_unban_pubkeys(writer.deref_mut(), response.result);
        Ok(())
    }

    fn on_send_raw_transaction(&self, response: SendRawTransactionResponse, raw_output: bool) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        wallet::on_send_raw_transaction(writer.deref_mut(), response, raw_output);
        Ok(())
    }

    fn on_withdraw(&self, response: WithdrawResponse, raw_output: bool) -> Result<()> {
        let mut writer = self.writer.borrow_mut();
        wallet::on_withdraw(writer.deref_mut(), response, raw_output)
    }
}
