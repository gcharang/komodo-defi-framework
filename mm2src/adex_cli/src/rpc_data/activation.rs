//! Contains rpc data layer structures that are not ready to become a part of the mm2_rpc::data module
//!
//! *Note: it's expected that the following data types will be moved to mm2_rpc::data when mm2 is refactored to be able to handle them*
//!

#[path = "activation/bch.rs"] pub(crate) mod bch;
#[path = "activation/electrum.rs"] mod electrum;
#[path = "activation/enable.rs"] mod enable;
#[path = "activation/eth.rs"] pub(crate) mod eth;
#[path = "activation/tendermint.rs"] pub(crate) mod tendermint;
#[path = "activation/zcoin.rs"] pub(crate) mod zcoin;

use rpc::v1::types::{Bytes as BytesJson, H256 as H256Json};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

use common::true_f;
use derive_more::Display;
use mm2_number::BigDecimal;
use mm2_rpc::data::legacy::{ElectrumProtocol, Mm2RpcResult};
use mm2_rpc::data::version2::MmRpcErrorV2;

use crate::rpc_data::activation::zcoin::ZcoinActivationParams;
use crate::rpc_data::eth::EthWithTokensActivationRequest;
use crate::rpc_data::Bip44Chain;

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "get_enabled_coins")]
pub(crate) struct GetEnabledRequest {}

pub(crate) enum ActivationMethod {
    Legacy(ActivationMethodLegacy),
    V2(ActivationMethodV2),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub(crate) enum ActivationMethodLegacy {
    Enable(enable::EnableRequest),
    Electrum(electrum::ElectrumRequest),
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub(crate) enum ActivationMethodV2 {
    EnableBchWithTokens(EnablePlatformCoinWithTokensReq<bch::BchWithTokensActivationParams>),
    EnableSlp(EnableTokenRequest<bch::SlpActivationRequest>),
    EnableTendermintWithAssets(EnablePlatformCoinWithTokensReq<tendermint::TendermintActivationParams>),
    EnableTendermintToken(EnableTokenRequest<tendermint::TendermintTokenActivationParams>),
    EnableEthWithTokens(EnablePlatformCoinWithTokensReq<EthWithTokensActivationRequest>),
    EnableErc20(EnableTokenRequest<eth::Erc20TokenActivationRequest>),
    #[serde(rename = "task::enable_z_coin::init")]
    EnableZCoin(InitStandaloneCoinReq<zcoin::ZcoinActivationParams>),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EnablePlatformCoinWithTokensReq<T: Serialize> {
    ticker: String,
    #[serde(flatten)]
    request: T,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TokenActivationRequest<Req> {
    ticker: String,
    #[serde(flatten)]
    request: Req,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EnableTokenRequest<T> {
    ticker: String,
    activation_params: T,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(tag = "method", rename = "disable_coin")]
pub(crate) struct DisableCoinRequest {
    pub(crate) coin: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum DisableCoinResponse {
    Success(Mm2RpcResult<DisableCoinSuccess>),
    Failed(DisableCoinFailed),
}

#[derive(Debug, Deserialize)]
pub(crate) struct DisableCoinSuccess {
    pub(crate) coin: String,
    pub(crate) cancelled_orders: Vec<Uuid>,
    pub(crate) passivized: bool,
}

#[derive(Deserialize)]
pub(crate) struct DisableCoinFailed {
    pub(crate) error: String,
    pub(crate) orders: DisableFailedOrders,
    pub(crate) active_swaps: Vec<Uuid>,
}

#[derive(Deserialize)]
pub(crate) struct DisableFailedOrders {
    pub(crate) matching: Vec<Uuid>,
    pub(crate) cancelled: Vec<Uuid>,
}

#[derive(Deserialize)]
pub(crate) struct SetRequiredConfResponse {
    pub(crate) coin: String,
    pub(crate) confirmations: u64,
}

#[derive(Deserialize)]
pub(crate) struct SetRequiredNotaResponse {
    pub(crate) coin: String,
    pub(crate) requires_notarization: bool,
}

#[derive(Default, Serialize)]
#[serde(tag = "method", rename = "coins_needed_for_kick_start")]
pub(crate) struct CoinsToKickStartRequest {}

pub(crate) type CoinsToKickstartResponse = Vec<String>;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CoinBalance {
    pub(crate) spendable: BigDecimal,
    pub(crate) unspendable: BigDecimal,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CoinAddressInfo<Balance> {
    pub(crate) derivation_method: DerivationMethod,
    pub(crate) pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) balances: Option<Balance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tickers: Option<HashSet<String>>,
}

pub(crate) type TokenBalances = HashMap<String, CoinBalance>;

#[derive(Display, Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum DerivationMethod {
    Iguana,
    #[allow(dead_code)]
    HDWallet(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ElectrumRpcRequest {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) protocol: ElectrumProtocol,
    #[serde(default)]
    pub(crate) disable_cert_verification: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct InitStandaloneCoinReq<T> {
    ticker: String,
    activation_params: T,
}

//--------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct InitRpcTaskResponse {
    pub(crate) task_id: TaskId,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RpcTaskStatusRequest {
    pub(crate) task_id: TaskId,
    #[serde(default = "true_f")]
    pub(crate) forget_if_finished: bool,
}

pub(crate) type TaskId = u64;

pub(crate) type InitStandaloneCoinResponse = InitRpcTaskResponse;
pub(crate) type InitStandaloneCoinStatusRequest = RpcTaskStatusRequest;
pub(crate) type InitStandaloneCoinStatusResponse = ZCoinStatus;
pub(crate) type InitStandaloneCoinStatusError = RpcTaskStatusError;

#[derive(Debug, Deserialize)]
#[serde(tag = "status", content = "details")]
pub(crate) enum RpcTaskStatus<Item, Error, InProgressStatus, AwaitingStatus> {
    Ok(Item),
    Error(Error),
    InProgress(InProgressStatus),
    UserActionRequired(AwaitingStatus),
}

type ZCoinStatus =
    RpcTaskStatus<ZcoinActivationResult, InitStandaloneCoinError, ZcoinInProgressStatus, ZcoinAwaitingStatus>;

#[derive(Display, Deserialize)]
#[serde(tag = "error_type", content = "error_data")]
pub(crate) enum RpcTaskStatusError {
    #[display(fmt = "No such task '{}'", _0)]
    NoSuchTask(TaskId),
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
}

#[derive(Display, Deserialize)]
#[serde(tag = "error_type", content = "error_data")]
pub(crate) enum InitStandaloneCoinError {
    #[display(fmt = "No such task '{}'", _0)]
    NoSuchTask(TaskId),
    #[display(fmt = "Initialization task has timed out {:?}", duration)]
    TaskTimedOut { duration: Duration },
    #[display(fmt = "Coin {} is activated already", ticker)]
    CoinIsAlreadyActivated { ticker: String },
    #[display(fmt = "Coin {} config is not found", _0)]
    CoinConfigIsNotFound(String),
    #[display(fmt = "Coin {} protocol parsing failed: {}", ticker, error)]
    CoinProtocolParseError { ticker: String, error: String },
    #[display(fmt = "Unexpected platform protocol {:?} for {}", protocol, ticker)]
    UnexpectedCoinProtocol { ticker: String, protocol: CoinProtocol },
    #[display(fmt = "Error on platform coin {} creation: {}", ticker, error)]
    CoinCreationError { ticker: String, error: String },
    #[display(fmt = "{}", _0)]
    HwError(HwRpcError),
    #[display(fmt = "Transport error: {}", _0)]
    Transport(String),
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
}

#[derive(Clone, Debug, Display, Serialize, Deserialize)]
pub(crate) enum HwRpcError {
    #[display(fmt = "No Trezor device available")]
    NoTrezorDeviceAvailable = 0,
    #[display(fmt = "Found multiple devices. Please unplug unused devices")]
    FoundMultipleDevices,
    #[display(fmt = "Found unexpected device. Please re-initialize Hardware wallet")]
    FoundUnexpectedDevice,
    #[display(fmt = "Pin is invalid")]
    InvalidPin,
    #[display(fmt = "Unexpected message")]
    UnexpectedMessage,
    #[display(fmt = "Button expected")]
    ButtonExpected,
    #[display(fmt = "Got data error")]
    DataError,
    #[display(fmt = "Pin expected")]
    PinExpected,
    #[display(fmt = "Invalid signature")]
    InvalidSignature,
    #[display(fmt = "Got process error")]
    ProcessError,
    #[display(fmt = "Not enough funds")]
    NotEnoughFunds,
    #[display(fmt = "Not initialized")]
    NotInitialized,
    #[display(fmt = "Wipe code mismatch")]
    WipeCodeMismatch,
    #[display(fmt = "Invalid session")]
    InvalidSession,
    #[display(fmt = "Got firmware error")]
    FirmwareError,
    #[display(fmt = "Failure message not found")]
    FailureMessageNotFound,
    #[display(fmt = "User cancelled action")]
    UserCancelled,
    #[display(fmt = "PONG message mismatch after ping")]
    PongMessageMismatch,
}

#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub(crate) enum ZcoinInProgressStatus {
    ActivatingCoin,
    UpdatingBlocksCache {
        current_scanned_block: u64,
        latest_block: u64,
    },
    BuildingWalletDb {
        current_scanned_block: u64,
        latest_block: u64,
    },
    TemporaryError(String),
    RequestingWalletBalance,
    Finishing,
    WaitingForTrezorToConnect,
    WaitingForUserToConfirmPubkey,
}

pub(crate) type ZcoinAwaitingStatus = HwRpcTaskAwaitingStatus;

#[derive(Deserialize)]
pub(crate) enum HwRpcTaskAwaitingStatus {
    EnterTrezorPin,
    EnterTrezorPassphrase,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "protocol_data")]
pub(crate) enum CoinProtocol {
    ZHTLC(ZcoinProtocolInfo),
}

#[derive(Debug, Deserialize)]
pub(crate) struct ZcoinProtocolInfo {
    consensus_params: ZcoinConsensusParams,
    check_point_block: Option<CheckPointBlockInfo>,
    z_derivation_path: Option<StandardHDPathToCoin>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckPointBlockInfo {
    height: u32,
    hash: H256Json,
    time: u32,
    sapling_tree: BytesJson,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ZcoinConsensusParams {
    overwinter_activation_height: u32,
    sapling_activation_height: u32,
    blossom_activation_height: Option<u32>,
    heartwood_activation_height: Option<u32>,
    canopy_activation_height: Option<u32>,
    coin_type: u32,
    hrp_sapling_extended_spending_key: String,
    hrp_sapling_extended_full_viewing_key: String,
    hrp_sapling_payment_address: String,
    b58_pubkey_address_prefix: [u8; 2],
    b58_script_address_prefix: [u8; 2],
}

#[rustfmt::skip]
pub(crate) type StandardHDPathToCoin =
    Bip32Child<Bip32PurposeValue, // `purpose`
    Bip32Child<HardenedValue, // `coin_type`
    Bip44Tail>>;

#[derive(Debug, Deserialize)]
pub(crate) struct Bip32Child<Value, Child> {
    value: Value,
    child: Child,
}

#[repr(u32)]
#[derive(Debug, Deserialize)]
pub(crate) enum Bip43Purpose {
    Bip32 = 32,
    Bip44 = 44,
    Bip49 = 49,
    Bip84 = 84,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Bip32PurposeValue {
    purpose: Bip43Purpose,
}

pub(crate) type HardenedValue = AnyValue<true>;

#[derive(Debug, Deserialize)]
pub(crate) struct AnyValue<const HARDENED: bool> {
    number: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Bip44Tail;

#[derive(Deserialize)]
pub(crate) struct ZcoinActivationResult {
    pub(crate) ticker: String,
    pub(crate) current_block: u64,
    pub(crate) wallet_balance: CoinBalanceReport,
}

#[derive(Deserialize)]
#[serde(tag = "wallet_type")]
pub(crate) enum CoinBalanceReport {
    Iguana(IguanaWalletBalance),
    HD(HDWalletBalance),
}

#[derive(Deserialize)]
pub(crate) struct IguanaWalletBalance {
    pub(crate) address: String,
    pub(crate) balance: CoinBalance,
}

#[derive(Deserialize)]
pub(crate) struct HDWalletBalance {
    pub(crate) accounts: Vec<HDAccountBalance>,
}

#[derive(Deserialize)]
pub(crate) struct HDAccountBalance {
    pub(crate) account_index: u32,
    pub(crate) derivation_path: RpcDerivationPath,
    pub(crate) total_balance: CoinBalance,
    pub(crate) addresses: Vec<HDAddressBalance>,
}

#[derive(Deserialize)]
pub(crate) struct HDAddressBalance {
    pub(crate) address: String,
    pub(crate) derivation_path: RpcDerivationPath,
    pub(crate) chain: Bip44Chain,
    pub(crate) balance: CoinBalance,
}

#[derive(Deserialize)]
pub(crate) struct RpcDerivationPath(pub(crate) DerivationPath);

#[derive(Deserialize)]
pub(crate) struct DerivationPath {
    pub(crate) path: Vec<ChildNumber>,
}

#[derive(Deserialize)]
pub(crate) struct ChildNumber(pub(crate) u32);
