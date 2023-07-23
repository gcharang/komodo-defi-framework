use clap::Args;
use rpc::v1::types::H256 as H256Json;
use std::mem::take;
use std::str::FromStr;

use crate::rpc_data::UnbanPubkeysRequest;
use mm2_rpc::data::legacy::{BanPubkeysRequest, UnbanPubkeysReq};

#[derive(Args)]
pub(crate) struct BanPubkeyArgs {
    #[arg(
        long,
        short,
        value_parser = H256Json::from_str,
        help = "Pubkey to ban"
    )]
    pubkey: H256Json,
    #[arg(long, short, help = "Reason of banning")]
    reason: String,
}

impl From<&mut BanPubkeyArgs> for BanPubkeysRequest {
    fn from(value: &mut BanPubkeyArgs) -> Self {
        BanPubkeysRequest {
            pubkey: take(&mut value.pubkey),
            reason: take(&mut value.reason),
        }
    }
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub(crate) struct UnbanPubkeysArgs {
    #[arg(
        long,
        short,
        group = "unban-pubkeys",
        default_value_t = false,
        help = "Whether to unban all pubkeys"
    )]
    pub(crate) all: bool,
    #[arg(
        long,
        short,
        group = "unban-pubkeys",
        value_parser = H256Json::from_str,
        help="Pubkey to unban"
    )]
    pub(crate) pubkey: Vec<H256Json>,
}

impl From<&mut UnbanPubkeysArgs> for UnbanPubkeysRequest {
    fn from(value: &mut UnbanPubkeysArgs) -> Self {
        UnbanPubkeysRequest {
            unban_by: if value.all {
                UnbanPubkeysReq::All
            } else {
                UnbanPubkeysReq::Few(take(&mut value.pubkey))
            },
        }
    }
}
