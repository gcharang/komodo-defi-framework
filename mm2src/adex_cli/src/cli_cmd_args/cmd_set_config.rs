use clap::Args;

#[derive(Args)]
#[group(required = true, multiple = true)]
pub(crate) struct SetConfigArgs {
    #[arg(long, help = "Set if you are going to set up a password")]
    pub(crate) password: bool,
    #[arg(long, name = "URI", help = "Adex RPC API Uri. http://localhost:7783")]
    pub(crate) uri: Option<String>,
}
