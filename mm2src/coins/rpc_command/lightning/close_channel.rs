use crate::{lp_coinfind_or_err, CoinFindError, MmCoinEnum};
use common::{async_blocking, HttpStatusCode};
use http::StatusCode;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use uuid::Uuid;

const CLOSE_CHANNEL_MSG_HELPER: &str =
    "To see if the channel is closed or not, you can use `lightning::channel::get_channel_details`";

type CloseChannelResult<T> = Result<T, MmError<CloseChannelError>>;

#[derive(Debug, Deserialize, Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum CloseChannelError {
    #[display(fmt = "Lightning network is not supported for {}", _0)]
    UnsupportedCoin(String),
    #[display(fmt = "No such coin {}", _0)]
    NoSuchCoin(String),
    #[display(fmt = "No such channel with uuid {}", _0)]
    NoSuchChannel(Uuid),
    #[display(fmt = "Closing channel error: {}", _0)]
    CloseChannelError(String),
}

impl HttpStatusCode for CloseChannelError {
    fn status_code(&self) -> StatusCode {
        match self {
            CloseChannelError::UnsupportedCoin(_) => StatusCode::BAD_REQUEST,
            CloseChannelError::NoSuchChannel(_) | CloseChannelError::NoSuchCoin(_) => StatusCode::NOT_FOUND,
            CloseChannelError::CloseChannelError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<CoinFindError> for CloseChannelError {
    fn from(e: CoinFindError) -> Self {
        match e {
            CoinFindError::NoSuchCoin { coin } => CloseChannelError::NoSuchCoin(coin),
        }
    }
}

#[derive(Deserialize)]
pub struct CloseChannelReq {
    pub coin: String,
    pub uuid: Uuid,
    #[serde(default)]
    pub force_close: bool,
}

#[derive(Display)]
pub enum CloseChannelResponseMessage {
    #[display(fmt = "Initiated closing of the channel. {}", CLOSE_CHANNEL_MSG_HELPER)]
    ChannelClosed,
    #[display(
        fmt = "Initiated force closing of the channel by broadcasting the latest local commitment transaction. {}",
        CLOSE_CHANNEL_MSG_HELPER
    )]
    ChannelForceClosed,
}

#[derive(Serialize)]
pub struct CloseChannelResponse {
    uuid: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    force_close_spend_delay: Option<u16>,
    message: String,
}

pub async fn close_channel(ctx: MmArc, req: CloseChannelReq) -> CloseChannelResult<CloseChannelResponse> {
    let ln_coin = match lp_coinfind_or_err(&ctx, &req.coin).await? {
        MmCoinEnum::LightningCoin(c) => c,
        e => return MmError::err(CloseChannelError::UnsupportedCoin(e.ticker().to_string())),
    };

    let channel_details = ln_coin
        .get_channel_by_uuid(req.uuid)
        .await
        .ok_or(CloseChannelError::NoSuchChannel(req.uuid))?;
    let channel_id = channel_details.channel_id;
    let counterparty_node_id = channel_details.counterparty.node_id;

    if req.force_close {
        async_blocking(move || {
            ln_coin
                .channel_manager
                .force_close_broadcasting_latest_txn(&channel_id, &counterparty_node_id)
                .map_to_mm(|e| CloseChannelError::CloseChannelError(format!("{:?}", e)))
        })
        .await?;
        Ok(CloseChannelResponse {
            uuid: req.uuid,
            force_close_spend_delay: channel_details.force_close_spend_delay,
            message: CloseChannelResponseMessage::ChannelForceClosed.to_string(),
        })
    } else {
        async_blocking(move || {
            ln_coin
                .channel_manager
                .close_channel(&channel_id, &counterparty_node_id)
                .map_to_mm(|e| CloseChannelError::CloseChannelError(format!("{:?}", e)))
        })
        .await?;
        Ok(CloseChannelResponse {
            uuid: req.uuid,
            force_close_spend_delay: None,
            message: CloseChannelResponseMessage::ChannelClosed.to_string(),
        })
    }
}
