use clap::Args;
use hex::FromHexError;
use rpc::v1::types::Bytes as BytesJson;
use std::mem::take;

use crate::rpc_data::SendRawTransactionRequest;

#[derive(Args)]
pub(crate) struct SendRawTransactionArgs {
    #[arg(long, short, help = "Name of the coin network on which to broadcast the transaction")]
    coin: String,
    #[arg(
        long,
        short,
        value_parser=parse_bytes,
        help="Transaction bytes in hexadecimal format;"
    )]
    tx_hex: BytesJson,
}

fn parse_bytes(value: &str) -> Result<BytesJson, FromHexError> {
    let bytes = hex::decode(value)?;
    Ok(BytesJson(bytes))
}

impl From<&mut SendRawTransactionArgs> for SendRawTransactionRequest {
    fn from(value: &mut SendRawTransactionArgs) -> Self {
        SendRawTransactionRequest {
            coin: take(&mut value.coin),
            tx_hex: take(&mut value.tx_hex),
        }
    }
}
