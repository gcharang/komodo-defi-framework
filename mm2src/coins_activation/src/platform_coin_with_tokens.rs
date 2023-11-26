use std::time::Duration;

use crate::context::CoinsActivationContext;
use crate::prelude::*;
use async_trait::async_trait;
use coins::my_tx_history_v2::TxHistoryStorage;
use coins::tx_history_storage::{CreateTxHistoryStorageError, TxHistoryStorageBuilder};
use coins::{lp_coinfind, lp_coinfind_any, CoinProtocol, CoinsContext, MmCoin, MmCoinEnum, PrivKeyPolicyNotAllowed,
            UnexpectedDerivationMethod};
use common::{log, HttpStatusCode, StatusCode};
use crypto::hw_rpc_task::{HwRpcTaskAwaitingStatus, HwRpcTaskUserAction};
use crypto::CryptoCtxError;
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_event_stream::EventStreamConfiguration;
use mm2_number::BigDecimal;
use rpc_task::rpc_common::{InitRpcTaskResponse, RpcTaskStatusRequest};
use rpc_task::{RpcTask, RpcTaskError, RpcTaskHandle, RpcTaskManager, RpcTaskManagerShared, RpcTaskStatus,
               RpcTaskTypes, TaskId};
use ser_error_derive::SerializeErrorType;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value as Json;

#[derive(Clone, Debug, Deserialize)]
pub struct TokenActivationRequest<Req> {
    ticker: String,
    #[serde(flatten)]
    request: Req,
}

pub trait TokenOf: Into<MmCoinEnum> {
    type PlatformCoin: TryPlatformCoinFromMmCoinEnum + PlatformWithTokensActivationOps + RegisterTokenInfo<Self> + Clone;
}

pub struct TokenActivationParams<Req, Protocol> {
    pub(crate) ticker: String,
    pub(crate) activation_request: Req,
    pub(crate) protocol: Protocol,
}

#[async_trait]
pub trait TokenInitializer {
    type Token: TokenOf;
    type TokenActivationRequest: Send;
    type TokenProtocol: TryFromCoinProtocol + Send;
    type InitTokensError: NotMmError;

    fn tokens_requests_from_platform_request(
        platform_request: &<<Self::Token as TokenOf>::PlatformCoin as PlatformWithTokensActivationOps>::ActivationRequest,
    ) -> Vec<TokenActivationRequest<Self::TokenActivationRequest>>;

    async fn enable_tokens(
        &self,
        params: Vec<TokenActivationParams<Self::TokenActivationRequest, Self::TokenProtocol>>,
    ) -> Result<Vec<Self::Token>, MmError<Self::InitTokensError>>;

    fn platform_coin(&self) -> &<Self::Token as TokenOf>::PlatformCoin;
}

#[async_trait]
pub trait TokenAsMmCoinInitializer: Send + Sync {
    type PlatformCoin;
    type ActivationRequest;

    async fn enable_tokens_as_mm_coins(
        &self,
        ctx: MmArc,
        request: &Self::ActivationRequest,
    ) -> Result<Vec<MmCoinEnum>, MmError<InitTokensAsMmCoinsError>>;
}

pub enum InitTokensAsMmCoinsError {
    TokenConfigIsNotFound(String),
    CouldNotFetchBalance(String),
    UnexpectedDerivationMethod(UnexpectedDerivationMethod),
    Internal(String),
    TokenProtocolParseError { ticker: String, error: String },
    UnexpectedTokenProtocol { ticker: String, protocol: CoinProtocol },
}

impl From<CoinConfWithProtocolError> for InitTokensAsMmCoinsError {
    fn from(err: CoinConfWithProtocolError) -> Self {
        match err {
            CoinConfWithProtocolError::ConfigIsNotFound(e) => InitTokensAsMmCoinsError::TokenConfigIsNotFound(e),
            CoinConfWithProtocolError::CoinProtocolParseError { ticker, err } => {
                InitTokensAsMmCoinsError::TokenProtocolParseError {
                    ticker,
                    error: err.to_string(),
                }
            },
            CoinConfWithProtocolError::UnexpectedProtocol { ticker, protocol } => {
                InitTokensAsMmCoinsError::UnexpectedTokenProtocol { ticker, protocol }
            },
        }
    }
}

pub trait RegisterTokenInfo<T: TokenOf> {
    fn register_token_info(&self, token: &T);
}

#[async_trait]
impl<T> TokenAsMmCoinInitializer for T
where
    T: TokenInitializer + Send + Sync,
    InitTokensAsMmCoinsError: From<T::InitTokensError>,
    (T::InitTokensError, InitTokensAsMmCoinsError): NotEqual,
{
    type PlatformCoin = <T::Token as TokenOf>::PlatformCoin;
    type ActivationRequest = <Self::PlatformCoin as PlatformWithTokensActivationOps>::ActivationRequest;

    async fn enable_tokens_as_mm_coins(
        &self,
        ctx: MmArc,
        request: &Self::ActivationRequest,
    ) -> Result<Vec<MmCoinEnum>, MmError<InitTokensAsMmCoinsError>> {
        let tokens_requests = T::tokens_requests_from_platform_request(request);
        let token_params = tokens_requests
            .into_iter()
            .map(|req| -> Result<_, MmError<CoinConfWithProtocolError>> {
                let (_, protocol): (_, T::TokenProtocol) = coin_conf_with_protocol(&ctx, &req.ticker)?;
                Ok(TokenActivationParams {
                    ticker: req.ticker,
                    activation_request: req.request,
                    protocol,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let tokens = self.enable_tokens(token_params).await?;
        for token in tokens.iter() {
            self.platform_coin().register_token_info(token);
        }
        Ok(tokens.into_iter().map(Into::into).collect())
    }
}

pub trait GetPlatformBalance {
    fn get_platform_balance(&self) -> Option<BigDecimal>;
}

#[async_trait]
pub trait PlatformWithTokensActivationOps: Into<MmCoinEnum> + Send + Sync + 'static {
    type ActivationRequest: Clone + Send + Sync + TxHistory;
    type PlatformProtocolInfo: TryFromCoinProtocol + Send;
    type ActivationResult: GetPlatformBalance + CurrentBlock + serde::Serialize + Send + Clone + Sync + 'static;
    type ActivationError: NotMmError + std::fmt::Debug + NotEqual + Into<EnablePlatformCoinWithTokensError>;

    type InProgressStatus: InitPlatformWithTokensInitialStatus + serde::Serialize + Clone + Send + Sync + 'static;
    type AwaitingStatus: serde::Serialize + Clone + Send + Sync + 'static;
    type UserAction: serde::de::DeserializeOwned + NotMmError + Send + Sync + 'static;

    /// Initializes the platform coin itself
    async fn enable_platform_coin(
        ctx: MmArc,
        ticker: String,
        coin_conf: Json,
        activation_request: Self::ActivationRequest,
        protocol_conf: Self::PlatformProtocolInfo,
    ) -> Result<Self, MmError<Self::ActivationError>>;

    fn try_from_mm_coin(coin: MmCoinEnum) -> Option<Self>
    where
        Self: Sized;

    fn token_initializers(
        &self,
    ) -> Vec<Box<dyn TokenAsMmCoinInitializer<PlatformCoin = Self, ActivationRequest = Self::ActivationRequest>>>;

    async fn get_activation_result(
        &self,
        task_handle: Option<&RpcTaskHandle<InitPlatformCoinWithTokensTask<Self>>>,
        activation_request: &Self::ActivationRequest,
    ) -> Result<Self::ActivationResult, MmError<Self::ActivationError>>
    where
        Self: MmCoin + Clone,
        EnablePlatformCoinWithTokensError: From<Self::ActivationError>;

    fn start_history_background_fetching(
        &self,
        ctx: MmArc,
        storage: impl TxHistoryStorage,
        initial_balance: Option<BigDecimal>,
    );

    async fn handle_balance_streaming(
        &self,
        config: &EventStreamConfiguration,
    ) -> Result<(), MmError<Self::ActivationError>>;
    
    fn rpc_task_manager(activation_ctx: &CoinsActivationContext) -> &InitPlatformTaskManagerShared<Self>
    where
        Self: MmCoin + Clone,
        EnablePlatformCoinWithTokensError: From<<Self as PlatformWithTokensActivationOps>::ActivationError>;
}

#[derive(Debug, Deserialize, Clone)]
pub struct EnablePlatformCoinWithTokensReq<T: Clone> {
    ticker: String,
    #[serde(flatten)]
    request: T,
}

#[derive(Debug, Display, Serialize, SerializeErrorType, Clone)]
#[serde(tag = "error_type", content = "error_data")]
pub enum EnablePlatformCoinWithTokensError {
    PlatformIsAlreadyActivated(String),
    #[display(fmt = "Platform {} config is not found", _0)]
    PlatformConfigIsNotFound(String),
    #[display(fmt = "Platform coin {} protocol parsing failed: {}", ticker, error)]
    CoinProtocolParseError {
        ticker: String,
        error: String,
    },
    #[display(fmt = "Unexpected platform protocol {:?} for {}", protocol, ticker)]
    UnexpectedPlatformProtocol {
        ticker: String,
        protocol: CoinProtocol,
    },
    #[display(fmt = "Token {} config is not found", _0)]
    TokenConfigIsNotFound(String),
    #[display(fmt = "Token {} protocol parsing failed: {}", ticker, error)]
    TokenProtocolParseError {
        ticker: String,
        error: String,
    },
    #[display(fmt = "Unexpected token protocol {:?} for {}", protocol, ticker)]
    UnexpectedTokenProtocol {
        ticker: String,
        protocol: CoinProtocol,
    },
    #[display(fmt = "Error on platform coin {} creation: {}", ticker, error)]
    PlatformCoinCreationError {
        ticker: String,
        error: String,
    },
    #[display(fmt = "Private key is not allowed: {}", _0)]
    PrivKeyPolicyNotAllowed(PrivKeyPolicyNotAllowed),
    #[display(fmt = "Unexpected derivation method: {}", _0)]
    UnexpectedDerivationMethod(String),
    Transport(String),
    AtLeastOneNodeRequired(String),
    InvalidPayload(String),
    Internal(String),
    #[display(fmt = "No such task '{}'", _0)]
    NoSuchTask(TaskId),
    #[display(fmt = "Initialization task has timed out {:?}", duration)]
    TaskTimedOut {
        duration: Duration,
    },
}

impl From<CoinConfWithProtocolError> for EnablePlatformCoinWithTokensError {
    fn from(err: CoinConfWithProtocolError) -> Self {
        match err {
            CoinConfWithProtocolError::ConfigIsNotFound(ticker) => {
                EnablePlatformCoinWithTokensError::PlatformConfigIsNotFound(ticker)
            },
            CoinConfWithProtocolError::UnexpectedProtocol { ticker, protocol } => {
                EnablePlatformCoinWithTokensError::UnexpectedPlatformProtocol { ticker, protocol }
            },
            CoinConfWithProtocolError::CoinProtocolParseError { ticker, err } => {
                EnablePlatformCoinWithTokensError::CoinProtocolParseError {
                    ticker,
                    error: err.to_string(),
                }
            },
        }
    }
}

impl From<InitTokensAsMmCoinsError> for EnablePlatformCoinWithTokensError {
    fn from(err: InitTokensAsMmCoinsError) -> Self {
        match err {
            InitTokensAsMmCoinsError::TokenConfigIsNotFound(ticker) => {
                EnablePlatformCoinWithTokensError::TokenConfigIsNotFound(ticker)
            },
            InitTokensAsMmCoinsError::TokenProtocolParseError { ticker, error } => {
                EnablePlatformCoinWithTokensError::TokenProtocolParseError { ticker, error }
            },
            InitTokensAsMmCoinsError::UnexpectedTokenProtocol { ticker, protocol } => {
                EnablePlatformCoinWithTokensError::UnexpectedTokenProtocol { ticker, protocol }
            },
            InitTokensAsMmCoinsError::Internal(e) => EnablePlatformCoinWithTokensError::Internal(e),
            InitTokensAsMmCoinsError::CouldNotFetchBalance(e) => EnablePlatformCoinWithTokensError::Transport(e),
            InitTokensAsMmCoinsError::UnexpectedDerivationMethod(e) => {
                EnablePlatformCoinWithTokensError::UnexpectedDerivationMethod(e.to_string())
            },
        }
    }
}

impl From<CreateTxHistoryStorageError> for EnablePlatformCoinWithTokensError {
    fn from(e: CreateTxHistoryStorageError) -> Self {
        match e {
            CreateTxHistoryStorageError::Internal(internal) => EnablePlatformCoinWithTokensError::Internal(internal),
        }
    }
}

impl From<CryptoCtxError> for EnablePlatformCoinWithTokensError {
    fn from(e: CryptoCtxError) -> Self { EnablePlatformCoinWithTokensError::Internal(e.to_string()) }
}

impl From<RpcTaskError> for EnablePlatformCoinWithTokensError {
    fn from(e: RpcTaskError) -> Self {
        match e {
            RpcTaskError::NoSuchTask(task_id) => EnablePlatformCoinWithTokensError::NoSuchTask(task_id),
            RpcTaskError::Timeout(duration) => EnablePlatformCoinWithTokensError::TaskTimedOut { duration },
            rpc_internal => EnablePlatformCoinWithTokensError::Internal(rpc_internal.to_string()),
        }
    }
}

impl HttpStatusCode for EnablePlatformCoinWithTokensError {
    fn status_code(&self) -> StatusCode {
        match self {
            EnablePlatformCoinWithTokensError::CoinProtocolParseError { .. }
            | EnablePlatformCoinWithTokensError::TokenProtocolParseError { .. }
            | EnablePlatformCoinWithTokensError::PlatformCoinCreationError { .. }
            | EnablePlatformCoinWithTokensError::PrivKeyPolicyNotAllowed(_)
            | EnablePlatformCoinWithTokensError::UnexpectedDerivationMethod(_)
            | EnablePlatformCoinWithTokensError::Transport(_)
            | EnablePlatformCoinWithTokensError::Internal(_)
            | EnablePlatformCoinWithTokensError::TaskTimedOut { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            EnablePlatformCoinWithTokensError::PlatformIsAlreadyActivated(_)
            | EnablePlatformCoinWithTokensError::PlatformConfigIsNotFound(_)
            | EnablePlatformCoinWithTokensError::TokenConfigIsNotFound(_)
            | EnablePlatformCoinWithTokensError::UnexpectedPlatformProtocol { .. }
            | EnablePlatformCoinWithTokensError::InvalidPayload { .. }
            | EnablePlatformCoinWithTokensError::AtLeastOneNodeRequired(_)
            | EnablePlatformCoinWithTokensError::UnexpectedTokenProtocol { .. }
            | EnablePlatformCoinWithTokensError::NoSuchTask(_) => StatusCode::BAD_REQUEST,
        }
    }
}

pub async fn re_enable_passive_platform_coin_with_tokens<Platform>(
    ctx: MmArc,
    platform_coin: Platform,
    req: EnablePlatformCoinWithTokensReq<Platform::ActivationRequest>,
) -> Result<Platform::ActivationResult, MmError<EnablePlatformCoinWithTokensError>>
where
    Platform: PlatformWithTokensActivationOps + MmCoin + Clone,
    EnablePlatformCoinWithTokensError: From<Platform::ActivationError>,
    (Platform::ActivationError, EnablePlatformCoinWithTokensError): NotEqual,
{
    let mut mm_tokens = Vec::new();
    for initializer in platform_coin.token_initializers() {
        let tokens = initializer.enable_tokens_as_mm_coins(ctx.clone(), &req.request).await?;
        mm_tokens.extend(tokens);
    }

    let activation_result = platform_coin.get_activation_result(None, &req.request).await?;
    log::info!("{} current block {}", req.ticker, activation_result.current_block());

    let coins_ctx = CoinsContext::from_ctx(&ctx).unwrap();
    coins_ctx
        .add_platform_with_tokens(platform_coin.clone().into(), mm_tokens)
        .await
        .mm_err(|e| EnablePlatformCoinWithTokensError::PlatformIsAlreadyActivated(e.ticker))?;

    Ok(activation_result)
}

pub async fn enable_platform_coin_with_tokens<Platform>(
    ctx: MmArc,
    req: EnablePlatformCoinWithTokensReq<Platform::ActivationRequest>,
) -> Result<Platform::ActivationResult, MmError<EnablePlatformCoinWithTokensError>>
where
    Platform: PlatformWithTokensActivationOps + MmCoin + Clone,
    EnablePlatformCoinWithTokensError: From<Platform::ActivationError>,
    (Platform::ActivationError, EnablePlatformCoinWithTokensError): NotEqual,
{
    enable_platform_coin_with_tokens_within_rpc::<Platform>(ctx, None, req).await
}

pub async fn enable_platform_coin_with_tokens_within_rpc<Platform>(
    ctx: MmArc,
    task_handle: Option<&RpcTaskHandle<InitPlatformCoinWithTokensTask<Platform>>>,
    req: EnablePlatformCoinWithTokensReq<Platform::ActivationRequest>,
) -> Result<Platform::ActivationResult, MmError<EnablePlatformCoinWithTokensError>>
where
    Platform: PlatformWithTokensActivationOps + MmCoin + Clone,
    EnablePlatformCoinWithTokensError: From<Platform::ActivationError>,
    (Platform::ActivationError, EnablePlatformCoinWithTokensError): NotEqual,
{
    if let Ok(Some(coin)) = lp_coinfind_any(&ctx, &req.ticker).await {
        if !coin.is_available() {
            if let Some(platform_coin) = Platform::try_from_mm_coin(coin.inner) {
                return re_enable_passive_platform_coin_with_tokens(ctx, platform_coin, req).await;
            }
        }

        return MmError::err(EnablePlatformCoinWithTokensError::PlatformIsAlreadyActivated(
            req.ticker,
        ));
    }

    let (platform_conf, platform_protocol) = coin_conf_with_protocol(&ctx, &req.ticker)?;

    let platform_coin = Platform::enable_platform_coin(
        ctx.clone(),
        req.ticker.clone(),
        platform_conf,
        req.request.clone(),
        platform_protocol,
    )
    .await?;

    let mut mm_tokens = Vec::new();
    for initializer in platform_coin.token_initializers() {
        let tokens = initializer.enable_tokens_as_mm_coins(ctx.clone(), &req.request).await?;
        mm_tokens.extend(tokens);
    }

    let activation_result = platform_coin.get_activation_result(task_handle, &req.request).await?;
    log::info!("{} current block {}", req.ticker, activation_result.current_block());

    if req.request.tx_history() {
        platform_coin.start_history_background_fetching(
            ctx.clone(),
            TxHistoryStorageBuilder::new(&ctx).build()?,
            activation_result.get_platform_balance(),
        );
    }

    if let Some(config) = &ctx.event_stream_configuration {
        platform_coin.handle_balance_streaming(config).await?;
    }

    let coins_ctx = CoinsContext::from_ctx(&ctx).unwrap();
    coins_ctx
        .add_platform_with_tokens(platform_coin.into(), mm_tokens)
        .await
        .mm_err(|e| EnablePlatformCoinWithTokensError::PlatformIsAlreadyActivated(e.ticker))?;

    Ok(activation_result)
}

pub struct InitPlatformCoinWithTokensTask<Platform: PlatformWithTokensActivationOps> {
    ctx: MmArc,
    request: EnablePlatformCoinWithTokensReq<Platform::ActivationRequest>,
    /*coin_conf: Json,
    protocol_info: Platform::PlatformProtocolInfo,*/
}

impl<Platform: PlatformWithTokensActivationOps> RpcTaskTypes for InitPlatformCoinWithTokensTask<Platform> {
    type Item = Platform::ActivationResult;
    type Error = EnablePlatformCoinWithTokensError;
    type InProgressStatus = Platform::InProgressStatus;
    type AwaitingStatus = Platform::AwaitingStatus;
    type UserAction = Platform::UserAction;
}

#[async_trait]
impl<Platform> RpcTask for InitPlatformCoinWithTokensTask<Platform>
where
    Platform: PlatformWithTokensActivationOps + MmCoin + Clone + Send + 'static,
    //Platform::ActivationError: Into<EnablePlatformCoinWithTokensError>,
    EnablePlatformCoinWithTokensError: From<<Platform as PlatformWithTokensActivationOps>::ActivationError>,
{
    fn initial_status(&self) -> Self::InProgressStatus {
        <Platform::InProgressStatus as InitPlatformWithTokensInitialStatus>::initial_status()
    }

    /// Try to disable the coin in case if we managed to register it already.
    async fn cancel(self) {}

    async fn run(&mut self, task_handle: &RpcTaskHandle<Self>) -> Result<Self::Item, MmError<Self::Error>> {
        enable_platform_coin_with_tokens_within_rpc::<Platform>(
            self.ctx.clone(),
            Some(task_handle),
            self.request.clone(),
        )
        .await
    }
}

pub trait InitPlatformWithTokensInitialStatus {
    fn initial_status() -> Self;
}

//use serde_derive::Serialize;

pub type InitPlatformCoinWithTokensStandardAwaitingStatus = HwRpcTaskAwaitingStatus;
pub type InitPlatformCoinWithTokensStandardUserAction = HwRpcTaskUserAction;
pub type EnablePlatformCoinWithTokensResponse = InitRpcTaskResponse;
pub type EnablePlatformCoinWithTokensStatusRequest = RpcTaskStatusRequest;

pub type InitPlatformTaskManagerShared<Platform> = RpcTaskManagerShared<InitPlatformCoinWithTokensTask<Platform>>;

#[derive(Clone, Serialize)]
pub enum InitPlatformCoinWithTokensStandardInProgressStatus {
    ActivatingCoin,
    SyncingBlockHeaders {
        current_scanned_block: u64,
        last_block: u64,
    },
    TemporaryError(String),
    RequestingWalletBalance,
    Finishing,
    /// This status doesn't require the user to send `UserAction`,
    /// but it tells the user that he should confirm/decline an address on his device.
    WaitingForTrezorToConnect,
    FollowHwDeviceInstructions,
}

impl InitPlatformWithTokensInitialStatus for InitPlatformCoinWithTokensStandardInProgressStatus {
    fn initial_status() -> Self { InitPlatformCoinWithTokensStandardInProgressStatus::ActivatingCoin }
}

pub async fn init_platform_coin_with_tokens<Platform>(
    ctx: MmArc,
    request: EnablePlatformCoinWithTokensReq<Platform::ActivationRequest>,
) -> MmResult<EnablePlatformCoinWithTokensResponse, EnablePlatformCoinWithTokensError>
where
    Platform: PlatformWithTokensActivationOps + MmCoin + /*TryFromCoinProtocol +*/ Send + Sync + 'static + Clone,
    Platform::InProgressStatus: InitPlatformWithTokensInitialStatus,
    EnablePlatformCoinWithTokensError: From<Platform::ActivationError>, // TODO: check if Into also works
    (Platform::ActivationError, EnablePlatformCoinWithTokensError): NotEqual,
{
    if let Ok(Some(_)) = lp_coinfind(&ctx, &request.ticker).await {
        return MmError::err(EnablePlatformCoinWithTokensError::PlatformIsAlreadyActivated(
            request.ticker,
        ));
    }

    //let (coin_conf, protocol_info) = coin_conf_with_protocol::<Platform>(&ctx, &request.ticker)?;

    let coins_act_ctx =
        CoinsActivationContext::from_ctx(&ctx).map_to_mm(EnablePlatformCoinWithTokensError::Internal)?;
    let spawner = ctx.spawner();
    let task = InitPlatformCoinWithTokensTask::<Platform> {
        ctx,
        request,
        //coin_conf,
        //protocol_info,
    };
    let task_manager = Platform::rpc_task_manager(&coins_act_ctx);

    let task_id = RpcTaskManager::spawn_rpc_task(task_manager, &spawner, task)
        .mm_err(|e| EnablePlatformCoinWithTokensError::Internal(e.to_string()))?;

    Ok(EnablePlatformCoinWithTokensResponse { task_id })
}

pub async fn init_platform_coin_with_tokens_status<Platform: PlatformWithTokensActivationOps>(
    ctx: MmArc,
    req: EnablePlatformCoinWithTokensStatusRequest,
) -> MmResult<
    RpcTaskStatus<
        Platform::ActivationResult,
        EnablePlatformCoinWithTokensError,
        Platform::InProgressStatus,
        Platform::AwaitingStatus,
    >,
    EnablePlatformCoinWithTokensError,
>
where
    Platform: PlatformWithTokensActivationOps + MmCoin + /*TryFromCoinProtocol +*/ Send + Sync + 'static + Clone,
    EnablePlatformCoinWithTokensError: From<Platform::ActivationError>, // + SerMmErrorType,
{
    let coins_act_ctx =
        CoinsActivationContext::from_ctx(&ctx).map_to_mm(EnablePlatformCoinWithTokensError::Internal)?;
    let mut task_manager = Platform::rpc_task_manager(&coins_act_ctx)
        .lock()
        .map_to_mm(|poison| EnablePlatformCoinWithTokensError::Internal(poison.to_string()))?;
    task_manager
        .task_status(req.task_id, req.forget_if_finished)
        .or_mm_err(|| EnablePlatformCoinWithTokensError::NoSuchTask(req.task_id))
        .map(|rpc_task| rpc_task.map_err(|e| e))
}

pub mod for_tests {
    use coins::MmCoin;
    use common::{executor::Timer, now_ms, wait_until_ms};
    use mm2_core::mm_ctx::MmArc;
    use mm2_err_handle::prelude::MmResult;
    use rpc_task::RpcTaskStatus;

    use super::{init_platform_coin_with_tokens, init_platform_coin_with_tokens_status,
                EnablePlatformCoinWithTokensError, EnablePlatformCoinWithTokensReq,
                EnablePlatformCoinWithTokensStatusRequest, InitPlatformWithTokensInitialStatus, NotEqual,
                PlatformWithTokensActivationOps};

    /// test helper to activate platform coin with waiting for the result
    pub async fn init_platform_coin_with_tokens_loop<Platform>(
        ctx: MmArc,
        request: EnablePlatformCoinWithTokensReq<Platform::ActivationRequest>,
    ) -> MmResult<Platform::ActivationResult, EnablePlatformCoinWithTokensError>
    where
        Platform: PlatformWithTokensActivationOps + MmCoin + Clone + Send + Sync + 'static,
        Platform::InProgressStatus: InitPlatformWithTokensInitialStatus,
        EnablePlatformCoinWithTokensError: From<Platform::ActivationError>,
        (Platform::ActivationError, EnablePlatformCoinWithTokensError): NotEqual,
    {
        let init_result = init_platform_coin_with_tokens::<Platform>(ctx.clone(), request)
            .await
            .unwrap();
        let timeout = wait_until_ms(150000);
        loop {
            if now_ms() > timeout {
                panic!("init_standalone_coin timed out");
            }
            let status_req = EnablePlatformCoinWithTokensStatusRequest {
                task_id: init_result.task_id,
                forget_if_finished: true,
            };
            let status_res = init_platform_coin_with_tokens_status::<Platform>(ctx.clone(), status_req).await;
            if let Ok(status) = status_res {
                match status {
                    RpcTaskStatus::Ok(result) => break Ok(result),
                    RpcTaskStatus::Error(e) => break Err(e),
                    _ => Timer::sleep(1.).await,
                }
            } else {
                panic!("could not get init_standalone_coin status");
            }
        }
    }
}
