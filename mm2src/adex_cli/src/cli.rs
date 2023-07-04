use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::adex_config::{get_config, set_config, AdexConfig};
use crate::adex_proc::{AdexProc, ResponseHandler};
use crate::scenarios::{get_status, init, start_process, stop_process};
use crate::transport::SlurpTransport;

use super::cli_cmd_args::*;

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
    Enable {
        #[arg(name = "COIN", help = "Coin to be included into the trading index")]
        coin: String,
    },
    #[command(visible_alias = "balance", about = "Get coin balance")]
    MyBalance(BalanceArgs),
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
        visible_alias = "history",
        about = "Return all orders whether active or inactive that match the selected filters"
    )]
    OrdersHistory(OrdersHistoryArgs),
    #[command(visible_alias = "update", about = "Update order on the orderbook")]
    UpdateMakerOrder(UpdateMakerOrderArgs),
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub(super) struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(super) async fn execute<P: ResponseHandler, Cfg: AdexConfig + 'static>(
        args: impl Iterator<Item = String>,
        config: &Cfg,
        printer: &P,
    ) -> Result<()> {
        let transport = config.rpc_uri().map(SlurpTransport::new);

        let proc = AdexProc {
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
            Command::Enable { coin } => proc.enable(coin).await?,
            Command::MyBalance(balance_args) => proc.get_balance(balance_args.into()).await?,
            Command::GetEnabled => proc.get_enabled().await?,
            Command::Orderbook(ref orderbook_args) => {
                proc.get_orderbook(orderbook_args.into(), orderbook_args.into()).await?
            },
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
        }
        Ok(())
    }
}

#[derive(Subcommand)]
enum ConfigSubcommand {
    #[command(about = "Set komodo adex cli configuration")]
    Set(SetConfigArgs),
    #[command(about = "Get komodo adex cli configuration")]
    Get,
}
