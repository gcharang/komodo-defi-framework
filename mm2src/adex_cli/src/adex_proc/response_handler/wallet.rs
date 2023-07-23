use rpc::v1::types::Bytes as BytesJson;
use std::io::Write;

use common::{write_safe::io::WriteSafeIO, write_safe_io, writeln_safe_io};

pub(super) fn on_send_raw_transaction(writer: &mut dyn Write, response: BytesJson) {
    writeln_safe_io!(writer, "{}", hex::encode(response.as_slice()));
}
