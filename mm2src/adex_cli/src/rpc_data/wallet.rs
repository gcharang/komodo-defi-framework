use rpc::v1::types::Bytes as BytesJson;
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "method", rename = "send_raw_transaction")]
pub(crate) struct SendRawTransactionRequest {
    pub(crate) coin: String,
    pub(crate) tx_hex: BytesJson,
}
