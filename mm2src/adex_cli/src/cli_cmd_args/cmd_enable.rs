use clap::Args;

#[derive(Args)]
pub(crate) struct EnableArgs {
    #[arg(help = "Coin to be included into the trading index")]
    pub(crate) coin: String,
    #[arg(
        long,
        short = 'k',
        visible_aliases = ["track", "keep", "progress"],
        default_value_t = 0,
        help = "Whether to keep progress on task based commands"
    )]
    pub(crate) keep_progress: u64,
}
