use anyhow::Result;
use clap::{Parser, Subcommand};
use mm2_rpc::data::legacy::SellBuyRequest;
use std::mem::take;
use uuid::Uuid;

use crate::adex_config::{get_config, set_config, AdexConfig};
use crate::adex_proc::{AdexProc, OrderbookConfig, ResponseHandler};
use crate::scenarios::{get_status, init, start_process, stop_process};
use crate::transport::SlurpTransport;

use super::cli_args::*;

const MM2_CONFIG_FILE_DEFAULT: &str = "MM2.json";
const COINS_FILE_DEFAULT: &str = "coins";

#[derive(Subcommand)]
enum Command {
    #[command(about = "Initialize a predefined coin set and configuration to start mm2 instance with")]
    Init {
        #[arg(long, help = "coin set file path", default_value = COINS_FILE_DEFAULT)]
        mm_coins_path: String,
        #[arg(long, help = "mm2 configuration file path", default_value = MM2_CONFIG_FILE_DEFAULT)]
        mm_conf_path: String,
    },
    #[command(about = "Start mm2 instance")]
    Start {
        #[arg(long, help = "mm2 configuration file path")]
        mm_conf_path: Option<String>,
        #[arg(long, help = "coin set file path")]
        mm_coins_path: Option<String>,
        #[arg(long, help = "log file path")]
        mm_log: Option<String>,
    },
    #[command(about = "Stop mm2 using API")]
    Stop,
    #[command(about = "Kill mm2 process")]
    Kill,
    #[command(about = "Get mm2 running status")]
    Status,
    #[command(about = "Gets version of intermediary mm2 service")]
    Version,
    #[command(subcommand, about = "To manage rpc_password and mm2 RPC URL")]
    Config(ConfigSubcommand),
    #[command(about = "Puts an asset to the trading index")]
    Enable {
        #[arg(name = "ASSET", help = "Asset to be included into the trading index")]
        asset: String,
    },
    #[command(about = "Gets balance of an asset")]
    Balance {
        #[arg(name = "ASSET", help = "Asset to get balance of")]
        asset: String,
    },
    #[command(about = "Lists activated assets")]
    GetEnabled,
    #[command(about = "Gets orderbook")]
    Orderbook(OrderbookArgs),
    #[command(about = "Gets orderbook depth")]
    OrderbookDepth(OrderbookDepthArgs),
    Sell(SellOrderArgs),
    Buy(BuyOrderArgs),
    SetPrice(SetPriceArgs),
    #[command(subcommand, about = "To cancel one or a group of orders")]
    Cancel(CancelSubcommand),
    OrderStatus {
        uuid: Uuid,
    },
    BestOrders(BestOrderArgs),
    #[command(about = "Get my orders")]
    MyOrders,
    #[command(about = "Returns all orders whether active or inactive that match the selected filters")]
    History(OrdersHistoryArgs),
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) async fn execute<P: ResponseHandler, Cfg: AdexConfig + 'static>(
        args: impl Iterator<Item = String>,
        config: &Cfg,
        printer: &P,
    ) -> Result<()> {
        let transport = SlurpTransport::new(config.rpc_uri()?);

        let proc = AdexProc {
            transport: &transport,
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
            Command::Status => get_status(),
            Command::Stop => proc.send_stop().await?,
            Command::Config(ConfigSubcommand::Set { set_password, adex_uri }) => {
                set_config(*set_password, adex_uri.take())
            },
            Command::Config(ConfigSubcommand::Get) => get_config(),
            Command::Enable { asset } => proc.enable(asset).await?,
            Command::Balance { asset } => proc.get_balance(asset).await?,
            Command::GetEnabled => proc.get_enabled().await?,
            Command::Orderbook(ref orderbook_args) => {
                proc.get_orderbook(
                    &orderbook_args.base,
                    &orderbook_args.rel,
                    OrderbookConfig::from(orderbook_args),
                )
                .await?
            },
            Command::Sell(SellOrderArgs { order_cli }) => proc.sell(SellBuyRequest::from(order_cli)).await?,
            Command::Buy(BuyOrderArgs { order_cli }) => proc.buy(SellBuyRequest::from(order_cli)).await?,
            Command::Cancel(CancelSubcommand::Order { uuid }) => proc.cancel_order(uuid).await?,
            Command::Cancel(CancelSubcommand::All) => proc.cancel_all_orders().await?,
            Command::Cancel(CancelSubcommand::ByPair { base, rel }) => {
                proc.cancel_by_pair(take(base), take(rel)).await?
            },
            Command::Cancel(CancelSubcommand::ByCoin { ticker }) => proc.cancel_by_coin(take(ticker)).await?,
            Command::OrderStatus { uuid } => proc.order_status(uuid).await?,
            Command::BestOrders(best_orders_args) => {
                let show_orig_tickets = best_orders_args.show_orig_tickets;
                proc.best_orders(best_orders_args.into(), show_orig_tickets).await?
            },
            Command::MyOrders => proc.my_orders().await?,
            Command::SetPrice(set_price_args) => proc.set_price(set_price_args.into()).await?,
            Command::OrderbookDepth(orderbook_depth_args) => proc.orderbook_depth(orderbook_depth_args.into()).await?,
            Command::History(history_args) => proc.orders_history(history_args.into(), history_args.into()).await?,
        }
        Ok(())
    }
}

#[derive(Subcommand)]
enum ConfigSubcommand {
    #[command(about = "Sets komodo adex cli configuration")]
    Set {
        #[arg(long, help = "Set if you are going to set up a password")]
        set_password: bool,
        #[arg(long, name = "URI", help = "Adex RPC API Uri. http://localhost:7783")]
        adex_uri: Option<String>,
    },
    #[command(about = "Gets komodo adex cli configuration")]
    Get,
}

#[derive(Subcommand)]
enum CancelSubcommand {
    #[command(about = "Cancels certain order by uuid")]
    Order {
        #[arg(help = "Order identifier")]
        uuid: Uuid,
    },
    #[command(about = "Cancels all orders of current node")]
    All,
    #[command(about = "Cancels all orders of specific pair")]
    ByPair {
        #[arg(help = "base coin of the pair; ")]
        base: String,
        #[arg(help = "rel coin of the pair; ")]
        rel: String,
    },
    #[command(about = "Cancels all orders using the coin ticker as base or rel")]
    ByCoin {
        #[arg(help = "order is cancelled if it uses ticker as base or rel")]
        ticker: String,
    },
}
