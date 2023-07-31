use anyhow::Result;
use clap::{Parser, Subcommand};
use std::mem::take;

use crate::komodefi_config::{get_config, set_config, KomodefiConfig};
use crate::komodefi_proc::{KomodefiProc, ResponseHandler};
use crate::scenarios::{get_status, init, start_process, stop_process};
use crate::transport::SlurpTransport;

use super::cli_cmd_args::prelude::*;

const MM2_CONFIG_FILE_DEFAULT: &str = "MM2.json";
const COINS_FILE_DEFAULT: &str = "coins";

#[derive(Subcommand)]
enum Command {
    #[command(about = "Initialize a predefined coin set and configuration to start mm2 instance with")]
    Init {
        #[arg(long, visible_alias = "coins", help = "coin set file path", default_value = COINS_FILE_DEFAULT)]
        mm_coins_path: String,
        #[arg(long, visible_alias = "conf", help = "mm2 configuration file path", default_value = MM2_CONFIG_FILE_DEFAULT)]
        mm_conf_path: String,
    },
    #[command(about = "Start mm2 instance")]
    Start {
        #[arg(long, visible_alias = "conf", help = "mm2 configuration file path")]
        mm_conf_path: Option<String>,
        #[arg(long, visible_alias = "coins", help = "coin set file path")]
        mm_coins_path: Option<String>,
        #[arg(long, visible_alias = "log", help = "log file path")]
        mm_log: Option<String>,
    },
    #[command(about = "Stop mm2 using API")]
    Stop,
    #[command(about = "Kill mm2 process")]
    Kill,
    #[command(about = "Check if mm2 is running")]
    Check,
    #[command(about = "Get version of intermediary mm2 service")]
    Version,
    #[command(subcommand, about = "Manage rpc_password and mm2 RPC URL")]
    Config(ConfigSubcommand),
    #[command(about = "Put a coin to the trading index")]
    Enable(EnableArgs),
    #[command(about = "Deactivates enabled coin and also cancels all active orders that use the selected coin.")]
    Disable(DisableCoinArgs),
    #[command(visible_alias = "balance", about = "Get coin balance")]
    MyBalance(MyBalanceArgs),
    #[command(visible_alias = "enabled", about = "List activated coins")]
    GetEnabled,
    #[command(visible_aliases = ["obook", "ob"], about = "Get orderbook")]
    Orderbook(OrderbookArgs),
    #[command(about = "Get orderbook depth")]
    OrderbookDepth(OrderbookDepthArgs),
    Sell(SellOrderArgs),
    Buy(BuyOrderArgs),
    SetPrice(SetPriceArgs),
    #[command(subcommand, about = "Cancel one or many orders")]
    Cancel(CancelSubcommand),
    #[command(
        visible_alias = "status",
        about = "Return the data of the order with the selected uuid created by the current node"
    )]
    OrderStatus(OrderStatusArgs),
    #[command(
        visible_alias = "best",
        about = "Return the best priced trades available on the orderbook"
    )]
    BestOrders(BestOrderArgs),
    #[command(about = "Get my orders", visible_aliases = ["my", "mine"])]
    MyOrders,
    #[command(
        visible_aliases = ["history", "filter"],
        about = "Return all orders whether active or inactive that match the selected filters"
    )]
    OrdersHistory(OrdersHistoryArgs),
    #[command(visible_alias = "update", about = "Update order on the orderbook")]
    UpdateMakerOrder(UpdateMakerOrderArgs),
    #[command(subcommand, visible_alias = "swap", about = "Swap related commands")]
    Swaps(SwapSubcommand),
    #[command(about = "Return the minimum required volume for buy/sell/setprice methods for the selected coin")]
    MinTradingVol {
        coin: String,
    },
    #[command(
        about = "Return the maximum available volume for buy/sell methods for selected coin. \
                 The result should be used as is for sell method or divided by price for buy method."
    )]
    MaxTakerVol {
        coin: String,
    },
    #[command(
        visible_alias = "to-kick",
        about = "Return the coins that should be activated to continue the interrupted swaps"
    )]
    CoinsToKickStart,
    #[command(
        visible_alias = "preimage",
        about = "Return the approximate fee amounts that are paid per the whole swap"
    )]
    TradePreimage(TradePreimageArgs),
    #[command(
        visible_alias = "gossip-mesh",
        about = "Return an array of peerIDs added to a topics' mesh for each known gossipsub topic"
    )]
    GetGossipMesh,
    #[command(
        visible_alias = "relay-mesh",
        about = "Return a list of peerIDs included in our local relay mesh"
    )]
    GetRelayMesh,
    #[command(
        visible_alias = "peer-topics",
        about = "Return a map of peerIDs to an array of the topics to which they are subscribed"
    )]
    GetGossipPeerTopics,
    #[command(
        visible_alias = "topic-peers",
        about = "Return a map of topics to an array of the PeerIDs which are subscribers"
    )]
    GetGossipTopicPeers,
    #[command(
        visible_alias = "my-peer-id",
        about = "Return your unique identifying Peer ID on the network"
    )]
    GetMyPeerId,
    #[command(
        visible_alias = "peers-info",
        about = "Return all connected peers with their multiaddresses"
    )]
    GetPeersInfo,
    #[command(
        visible_alias = "set-conf",
        about = "Set the number of confirmations to wait for the selected coin"
    )]
    SetRequiredConf(SetRequiredConfArgs),
    #[command(
        visible_alias = "set-nota",
        about = "Whether to wait for a dPoW notarization of the given atomic swap transactions"
    )]
    SetRequiredNota(SetRequiredNotaArgs),
    #[command(
        visible_alias = "ban",
        about = "Bans the selected pubkey ignoring its order matching messages and preventing its \
                 orders from displaying in the orderbook. \
                 Use the secp256k1 pubkey without prefix for this method input"
    )]
    BanPubkey(BanPubkeyArgs),
    #[command(
        visible_aliases = ["ban-list", "list-banned"],
        about = "Returns a list of public keys of nodes that are banned from interacting with the node executing the method"
    )]
    ListBannedPubkeys,
    #[command(
        visible_alias = "unban",
        about = "Remove all currently banned pubkeys from ban list, or specific pubkeys"
    )]
    UnbanPubkeys(UnbanPubkeysArgs),
    #[command(
        visible_alias = "send-raw",
        about = "Broadcasts the transaction to the network of selected coin"
    )]
    SendRawTransaction(SendRawTransactionArgs),
    #[command(
        about = "Generates, signs, and returns a transaction that transfers the amount of coin to the address indicated in the to argument"
    )]
    Withdraw(WithdrawArgs),
    #[command(
        visible_aliases = ["get-public", "public-key", "public"],
        about = "Returns the compressed secp256k1 pubkey corresponding to the user's seed phrase"
    )]
    GetPublicKey,
    #[command(
        visible_aliases = ["pubkey-hash", "hash", "pubhash"],
        about = "Returns the RIPEMD-160 hash version of your public key"
    )]
    GetPublicKeyHash,
    #[command(
        visible_aliases = ["get-raw", "raw-tx", "raw"],
        about = "Returns the full signed raw transaction hex for any transaction that is confirmed or within the mempool"
    )]
    GetRawTransaction(GetRawTransactionArgs),
    #[command(subcommand, about = "Tracking the status of long-running commands")]
    Task(TaskSubcommand),
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(super) async fn execute<P: ResponseHandler, Cfg: KomodefiConfig + 'static>(
        args: impl Iterator<Item = String>,
        config: &Cfg,
        printer: &P,
    ) -> Result<()> {
        let transport = config.rpc_uri().map(SlurpTransport::new);

        let proc = KomodefiProc {
            transport: transport.as_ref(),
            response_handler: printer,
            config,
        };

        let mut parsed_cli = Self::parse_from(args);
        match &mut parsed_cli.command {
            Command::Init {
                mm_coins_path: coins_file,
                mm_conf_path: mm2_cfg_file,
            } => init(mm2_cfg_file, coins_file).await,
            Command::Start {
                mm_conf_path: mm2_cfg_file,
                mm_coins_path: coins_file,
                mm_log: log_file,
            } => start_process(mm2_cfg_file, coins_file, log_file),
            Command::Version => proc.get_version().await?,
            Command::Kill => stop_process(),
            Command::Check => get_status(),
            Command::Stop => proc.send_stop().await?,
            Command::Config(ConfigSubcommand::Set(SetConfigArgs { password, uri })) => {
                set_config(*password, uri.take())?
            },
            Command::Config(ConfigSubcommand::Get) => get_config(),
            Command::Enable(args) => proc.enable(&args.coin, args.keep_progress).await?,
            Command::Disable(args) => proc.disable(args.into()).await?,
            Command::MyBalance(my_balance_args) => proc.get_balance(my_balance_args.into()).await?,
            Command::GetEnabled => proc.get_enabled().await?,
            Command::Orderbook(obook_args) => proc.get_orderbook(obook_args.into(), obook_args.into()).await?,
            Command::Sell(sell_args) => proc.sell(sell_args.into()).await?,
            Command::Buy(buy_args) => proc.buy(buy_args.into()).await?,
            Command::Cancel(CancelSubcommand::Order(args)) => proc.cancel_order(args.into()).await?,
            Command::Cancel(CancelSubcommand::All) => proc.cancel_all_orders().await?,
            Command::Cancel(CancelSubcommand::ByPair(args)) => proc.cancel_by_pair(args.into()).await?,
            Command::Cancel(CancelSubcommand::ByCoin(args)) => proc.cancel_by_coin(args.into()).await?,
            Command::OrderStatus(order_status_args) => proc.order_status(order_status_args.into()).await?,
            Command::BestOrders(best_orders_args) => {
                let show_orig_tickets = best_orders_args.show_orig_tickets;
                proc.best_orders(best_orders_args.into(), show_orig_tickets).await?
            },
            Command::MyOrders => proc.my_orders().await?,
            Command::SetPrice(set_price_args) => proc.set_price(set_price_args.into()).await?,
            Command::OrderbookDepth(orderbook_depth_args) => proc.orderbook_depth(orderbook_depth_args.into()).await?,
            Command::OrdersHistory(history_args) => {
                proc.orders_history(history_args.into(), history_args.into()).await?
            },
            Command::UpdateMakerOrder(update_maker_args) => proc.update_maker_order(update_maker_args.into()).await?,
            Command::Swaps(SwapSubcommand::ActiveSwaps(args)) => {
                proc.active_swaps(args.include_status, args.uuids_only).await?
            },
            Command::Swaps(SwapSubcommand::MySwapStatus(args)) => proc.swap_status(args.uuid).await?,
            Command::Swaps(SwapSubcommand::MyRecentSwaps(args)) => proc.recent_swaps(args.into()).await?,
            Command::Swaps(SwapSubcommand::RecoverFundsOfSwap(args)) => proc.recover_funds_of_swap(args.into()).await?,
            Command::MinTradingVol { coin } => proc.min_trading_vol(take(coin)).await?,
            Command::MaxTakerVol { coin } => proc.max_taker_vol(take(coin)).await?,
            Command::TradePreimage(args) => proc.trade_preimage(args.into()).await?,
            Command::CoinsToKickStart => proc.coins_to_kick_start().await?,
            Command::GetGossipMesh => proc.get_gossip_mesh().await?,
            Command::GetRelayMesh => proc.get_relay_mesh().await?,
            Command::GetGossipPeerTopics => proc.get_gossip_peer_topics().await?,
            Command::GetGossipTopicPeers => proc.get_gossip_topic_peers().await?,
            Command::GetMyPeerId => proc.get_my_peer_id().await?,
            Command::GetPeersInfo => proc.get_peers_info().await?,
            Command::SetRequiredConf(args) => proc.set_required_confirmations(args.into()).await?,
            Command::SetRequiredNota(args) => proc.set_required_nota(args.into()).await?,
            Command::BanPubkey(args) => proc.ban_pubkey(args.into()).await?,
            Command::ListBannedPubkeys => proc.list_banned_pubkeys().await?,
            Command::UnbanPubkeys(args) => proc.unban_pubkeys(args.into()).await?,
            Command::SendRawTransaction(args) => proc.send_raw_transaction(args.into(), args.bare_output).await?,
            Command::Withdraw(args) => proc.withdraw(args.into(), args.bare_output).await?,
            Command::GetPublicKey => proc.get_public_key().await?,
            Command::GetPublicKeyHash => proc.get_public_key_hash().await?,
            Command::GetRawTransaction(args) => proc.get_raw_transaction(args.into(), args.bare_output).await?,
            Command::Task(TaskSubcommand::Status(TaskSubcommandStatus::Zcoin { task_id })) => {
                proc.enable_zcoin_status(*task_id, None).await?
            },
            Command::Task(TaskSubcommand::Cancel(TaskSubcommandCancel::Zcoin { task_id })) => {
                proc.enable_zcoin_cancel(*task_id).await?
            },
        }
        Ok(())
    }
}

#[derive(Subcommand)]
enum ConfigSubcommand {
    #[command(about = "Set komodo komodefi cli configuration")]
    Set(SetConfigArgs),
    #[command(about = "Get komodo komodefi cli configuration")]
    Get,
}
