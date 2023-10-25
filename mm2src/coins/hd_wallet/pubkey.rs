use super::*;
use async_trait::async_trait;
use crypto::hw_rpc_task::HwConnectStatuses;
use crypto::trezor::trezor_rpc_task::{TrezorRpcTaskProcessor, TryIntoUserAction};
use crypto::trezor::utxo::IGNORE_XPUB_MAGIC;
use crypto::trezor::ProcessTrezorResponse;
use crypto::{CryptoCtx, DerivationPath, EcdsaCurve, HardwareWalletArc, XPub, XPubConverter};
use mm2_core::mm_ctx::MmArc;
use rpc_task::{RpcTask, RpcTaskHandle};

const SHOW_PUBKEY_ON_DISPLAY: bool = false;

#[async_trait]
pub trait ExtractExtendedPubkey {
    type ExtendedPublicKey;

    async fn extract_extended_pubkey<XPubExtractor>(
        &self,
        xpub_extractor: Option<XPubExtractor>,
        derivation_path: DerivationPath,
    ) -> MmResult<Self::ExtendedPublicKey, HDExtractPubkeyError>
    where
        XPubExtractor: HDXPubExtractor + Send;
}

#[async_trait]
pub trait HDXPubExtractor: Sync {
    async fn extract_xpub(
        &self,
        trezor_coin: String,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError>;
}

// Todo: it would be good to separate hardware wallet specific code from HD wallet code.
pub enum RpcTaskXPubExtractor<'task, Task: RpcTask> {
    Trezor {
        hw_ctx: HardwareWalletArc,
        task_handle: &'task RpcTaskHandle<Task>,
        statuses: HwConnectStatuses<Task::InProgressStatus, Task::AwaitingStatus>,
    },
}

#[async_trait]
impl<'task, Task> HDXPubExtractor for RpcTaskXPubExtractor<'task, Task>
where
    Task: RpcTask,
    Task::UserAction: TryIntoUserAction + Send,
{
    async fn extract_xpub(
        &self,
        trezor_coin: String,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError> {
        match self {
            RpcTaskXPubExtractor::Trezor {
                hw_ctx,
                task_handle,
                statuses,
            } => Self::extract_xpub_from_trezor(hw_ctx, task_handle, statuses, trezor_coin, derivation_path).await,
        }
    }
}

impl<'task, Task> RpcTaskXPubExtractor<'task, Task>
where
    Task: RpcTask,
    Task::UserAction: TryIntoUserAction + Send,
{
    pub fn new(
        ctx: &MmArc,
        task_handle: &'task RpcTaskHandle<Task>,
        statuses: HwConnectStatuses<Task::InProgressStatus, Task::AwaitingStatus>,
    ) -> MmResult<RpcTaskXPubExtractor<'task, Task>, HDExtractPubkeyError> {
        let crypto_ctx = CryptoCtx::from_ctx(ctx)?;
        let hw_ctx = crypto_ctx
            .hw_ctx()
            .or_mm_err(|| HDExtractPubkeyError::HwContextNotInitialized)?;
        Ok(RpcTaskXPubExtractor::Trezor {
            hw_ctx,
            task_handle,
            statuses,
        })
    }

    async fn extract_xpub_from_trezor(
        hw_ctx: &HardwareWalletArc,
        task_handle: &RpcTaskHandle<Task>,
        statuses: &HwConnectStatuses<Task::InProgressStatus, Task::AwaitingStatus>,
        trezor_coin: String,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError> {
        let mut trezor_session = hw_ctx.trezor().await?;

        let pubkey_processor = TrezorRpcTaskProcessor::new(task_handle, statuses.to_trezor_request_statuses());
        let xpub = trezor_session
            .get_public_key(
                derivation_path,
                trezor_coin,
                EcdsaCurve::Secp256k1,
                SHOW_PUBKEY_ON_DISPLAY,
                IGNORE_XPUB_MAGIC,
            )
            .await?
            .process(&pubkey_processor)
            .await?;

        // Despite we pass `IGNORE_XPUB_MAGIC` to the [`TrezorSession::get_public_key`] method,
        // Trezor sometimes returns pubkeys with magic prefixes like `dgub` prefix for DOGE coin.
        // So we need to replace the magic prefix manually.
        XPubConverter::replace_magic_prefix(xpub).mm_err(HDExtractPubkeyError::from)
    }
}

/// This is a wrapper over `XPubExtractor`. The main goal of this structure is to allow construction of an Xpub extractor
/// even if HD wallet is not supported. But if someone tries to extract an Xpub despite HD wallet is not supported,
/// it fails with an inner `HDExtractPubkeyError` error.
pub struct XPubExtractorUnchecked<XPubExtractor>(MmResult<XPubExtractor, HDExtractPubkeyError>);

#[async_trait]
impl<XPubExtractor> HDXPubExtractor for XPubExtractorUnchecked<XPubExtractor>
where
    XPubExtractor: HDXPubExtractor + Send + Sync,
{
    async fn extract_xpub(
        &self,
        trezor_coin: String,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError> {
        self.0
            .as_ref()
            .map_err(Clone::clone)?
            .extract_xpub(trezor_coin, derivation_path)
            .await
    }
}
