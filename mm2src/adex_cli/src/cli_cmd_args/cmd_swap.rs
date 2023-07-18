use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub(crate) enum SwapSubcommand {
    #[command(
        short_flag = 'a',
        visible_alias = "active",
        about = "Get all the swaps that are currently running on the Komodo DeFi Framework node"
    )]
    ActiveSwaps(ActiveSwapsArgs),
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
}
