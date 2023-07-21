use clap::{Args, Subcommand};
use mm2_rpc::data::legacy::MySwapsFilter;
use uuid::Uuid;

use super::parse_datetime;
use crate::rpc_data::{MyRecentSwapsRequest, Params, RecoverFundsOfSwapRequest};

#[derive(Subcommand)]
pub(crate) enum SwapSubcommand {
    #[command(
        short_flag = 'a',
        visible_alias = "active",
        about = "Get all the swaps that are currently running"
    )]
    ActiveSwaps(ActiveSwapsArgs),
    #[command(
        short_flag = 's',
        visible_alias = "status",
        about = "Return the data of an atomic swap"
    )]
    MySwapStatus(MySwapStatusArgs),
    #[command(
        short_flag = 'r',
        visible_alias = "recent",
        about = "Return the data of the most recent atomic swaps by filter"
    )]
    MyRecentSwaps(MyRecentSwapsArgs),
    #[command(
        short_flag = 'R',
        visible_aliases = ["recover", "recover-funds", "refund"],
        about = "Reclaim the user funds from the swap-payment address, if possible"
    )]
    RecoverFundsOfSwap(RecoverFundsOfSwapArgs),
}

#[derive(Args, Debug)]
pub(crate) struct ActiveSwapsArgs {
    #[arg(
        long,
        short = 's',
        default_value_t = false,
        help = "Whether to include swap statuses in response; defaults to false"
    )]
    pub(crate) include_status: bool,
    #[arg(
        long,
        short = 'u',
        visible_alias = "uuids",
        default_value_t = false,
        help = "Whether to show only uuids of active swaps"
    )]
    pub(crate) uuids_only: bool,
}

#[derive(Args, Debug)]
pub(crate) struct MySwapStatusArgs {
    #[arg(help = "Uuid of swap, typically received from the buy/sell call")]
    pub(crate) uuid: Uuid,
}

#[derive(Args, Debug)]
pub(crate) struct MyRecentSwapsArgs {
    #[arg(
        long,
        short = 'l',
        default_value_t = 10,
        help = "Limits the number of returned swaps"
    )]
    pub(crate) limit: usize,
    #[arg(
        long,
        short = 'u',
        help = "Skip records until this uuid, skipping the from_uuid as well"
    )]
    pub(crate) from_uuid: Option<Uuid>,
    #[arg(
        long,
        short = 'p',
        help = "Return limit swaps from the selected page; This param will be ignored if from_uuid is set"
    )]
    pub(crate) page_number: Option<usize>,
    #[arg(
        long,
        short = 'm',
        visible_alias = "mine",
        help = "Return only swaps that match the swap.my_coin = request.my_coin condition"
    )]
    pub(crate) my_coin: Option<String>,
    #[arg(
        long,
        short = 'o',
        visible_alias = "other",
        help = "Return only swaps that match the swap.other_coin = request.other_coin condition"
    )]
    pub(crate) other_coin: Option<String>,
    #[arg(
        long,
        short = 't',
        value_parser = parse_datetime,
        help = "Return only swaps that match the swap.started_at >= request.from_timestamp condition. Datetime fmt: \"%y-%m-%dT%H:%M:%S\""
    )]
    pub(crate) from_timestamp: Option<u64>,
    #[arg(
        long,
        short = 'T',
        value_parser = parse_datetime,
        help = "Return only swaps that match the swap.started_at < request.to_timestamp condition. Datetime fmt: \"%y-%m-%dT%H:%M:%S\""
    )]
    pub(crate) to_timestamp: Option<u64>,
}

impl From<&mut MyRecentSwapsArgs> for MyRecentSwapsRequest {
    fn from(value: &mut MyRecentSwapsArgs) -> Self {
        MyRecentSwapsRequest {
            limit: value.limit,
            from_uuid: value.from_uuid.take(),
            page_number: value.page_number.take(),
            filter: MySwapsFilter {
                my_coin: value.my_coin.take(),
                other_coin: value.other_coin.take(),
                from_timestamp: value.from_timestamp.take(),
                to_timestamp: value.to_timestamp.take(),
            },
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct RecoverFundsOfSwapArgs {
    #[arg(help = "uuid of the swap to recover the funds")]
    pub(crate) uuid: Uuid,
}

impl From<&mut RecoverFundsOfSwapArgs> for RecoverFundsOfSwapRequest {
    fn from(value: &mut RecoverFundsOfSwapArgs) -> Self {
        RecoverFundsOfSwapRequest {
            params: Params { uuid: value.uuid },
        }
    }
}
