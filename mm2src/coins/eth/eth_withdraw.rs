use super::{checksum_address, get_addr_nonce, get_eth_gas_details, pubkey_from_xpub_str, u256_to_big_decimal,
            wei_from_big_decimal, EthCoinType, EthPrivKeyPolicy, WithdrawError, WithdrawRequest, WithdrawResult,
            ERC20_CONTRACT};
use crate::eth::{Action, EthTxFeeDetails, KeyPair, SignedEthTx, UnSignedEthTx};
use crate::rpc_command::init_withdraw::{WithdrawInProgressStatus, WithdrawTaskHandle};
use crate::{BytesJson, EthCoin, TransactionDetails};
use async_trait::async_trait;
use bip32::DerivationPath;
use common::custom_futures::timeout::FutureTimerExt;
use common::now_sec;
use crypto::{CryptoCtx, HwRpcError};
use ethabi::Token;
use ethkey::public_to_address;
use futures::compat::Future01CompatExt;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::{MapToMmResult, MmError, OrMmError};
use std::ops::Deref;

cfg_wasm32! {
    use web3::types::TransactionRequest;
}

#[async_trait]
pub trait EthWithdraw
where
    Self: Sized + Sync,
{
    fn coin(&self) -> &EthCoin;

    fn request(&self) -> &WithdrawRequest;

    #[allow(clippy::result_large_err)]
    fn on_generating_transaction(&self) -> Result<(), MmError<WithdrawError>>;

    #[allow(clippy::result_large_err)]
    fn on_finishing(&self) -> Result<(), MmError<WithdrawError>>;

    async fn sign_tx_with_trezor(
        &self,
        derivation_path: DerivationPath,
        unsigned_tx: &UnSignedEthTx,
    ) -> Result<SignedEthTx, MmError<WithdrawError>>;

    async fn build(self) -> WithdrawResult {
        let coin = self.coin();
        let ticker = coin.deref().ticker.clone();
        let req = self.request().clone();

        let to_addr = coin
            .address_from_str(&req.to)
            .map_to_mm(WithdrawError::InvalidAddress)?;
        let (my_balance, my_address, key_pair, derivation_path) = match req.from {
            Some(from) => {
                let path_to_coin = &coin
                    .deref()
                    .derivation_method
                    .hd_wallet()
                    .ok_or(WithdrawError::UnexpectedDerivationMethod)?
                    .derivation_path;
                let path_to_address = from.to_address_path(path_to_coin.coin_type())?;
                let derivation_path = path_to_address.to_derivation_path(path_to_coin)?;
                let (key_pair, address) = match coin.priv_key_policy {
                    EthPrivKeyPolicy::Trezor {
                        ref activated_pubkey, ..
                    } => {
                        let my_pubkey = activated_pubkey
                            .as_ref()
                            .or_mm_err(|| WithdrawError::InternalError("empty trezor xpub".to_string()))?;
                        let my_pubkey = pubkey_from_xpub_str(my_pubkey)
                            .map_to_mm(|_| WithdrawError::InternalError("invalid trezor xpub".to_string()))?;
                        let address = public_to_address(&my_pubkey);
                        (None, address)
                    },
                    _ => {
                        let raw_priv_key = coin
                            .priv_key_policy
                            .hd_wallet_derived_priv_key_or_err(&derivation_path)?;

                        let key_pair = KeyPair::from_secret_slice(raw_priv_key.as_slice())
                            .map_to_mm(|e| WithdrawError::InternalError(e.to_string()))?;

                        let address = key_pair.address();
                        (Some(key_pair), address)
                    },
                };
                let balance = coin.address_balance(address).compat().await?;
                (balance, address, key_pair, Some(derivation_path))
            },
            None => {
                let my_address = coin.derivation_method.single_addr_or_err().await?;
                (
                    coin.my_balance().compat().await?,
                    my_address,
                    Some(coin.priv_key_policy.activated_key_or_err()?.clone()),
                    None,
                )
            },
        };
        let my_balance_dec = u256_to_big_decimal(my_balance, coin.decimals)?;

        let (mut wei_amount, dec_amount) = if req.max {
            (my_balance, my_balance_dec.clone())
        } else {
            let wei_amount = wei_from_big_decimal(&req.amount, coin.decimals)?;
            (wei_amount, req.amount.clone())
        };
        if wei_amount > my_balance {
            return MmError::err(WithdrawError::NotSufficientBalance {
                coin: coin.ticker.clone(),
                available: my_balance_dec.clone(),
                required: dec_amount,
            });
        };
        let (mut eth_value, data, call_addr, fee_coin) = match &coin.coin_type {
            EthCoinType::Eth => (wei_amount, vec![], to_addr, ticker.as_str()),
            EthCoinType::Erc20 { platform, token_addr } => {
                let function = ERC20_CONTRACT.function("transfer")?;
                let data = function.encode_input(&[Token::Address(to_addr), Token::Uint(wei_amount)])?;
                (0.into(), data, *token_addr, platform.as_str())
            },
        };
        let eth_value_dec = u256_to_big_decimal(eth_value, coin.decimals)?;

        let (gas, gas_price) = get_eth_gas_details(
            coin,
            req.fee,
            eth_value,
            data.clone().into(),
            my_address,
            call_addr,
            false,
        )
        .await?;
        let total_fee = gas * gas_price;
        let total_fee_dec = u256_to_big_decimal(total_fee, coin.decimals)?;

        if req.max && coin.coin_type == EthCoinType::Eth {
            if eth_value < total_fee || wei_amount < total_fee {
                return MmError::err(WithdrawError::AmountTooLow {
                    amount: eth_value_dec,
                    threshold: total_fee_dec,
                });
            }
            eth_value -= total_fee;
            wei_amount -= total_fee;
        };

        let _nonce_lock = coin.nonce_lock.lock().await;
        let (nonce, _) = get_addr_nonce(my_address, coin.web3_instances.clone())
            .compat()
            .timeout_secs(30.)
            .await?
            .map_to_mm(WithdrawError::Transport)?;

        let tx = UnSignedEthTx {
            nonce,
            value: eth_value,
            action: Action::Call(call_addr),
            data: data.clone(),
            gas,
            gas_price,
        };

        let (tx_hash, tx_hex) = match coin.priv_key_policy {
            EthPrivKeyPolicy::Iguana(_) | EthPrivKeyPolicy::HDWallet { .. } => {
                let key_pair = key_pair.ok_or_else(|| WithdrawError::InternalError("no keypair found".to_string()))?;
                // Todo: nonce_lock is still global for all addresses but this needs to be per address
                let signed = tx.sign(key_pair.secret(), coin.chain_id);
                let bytes = rlp::encode(&signed);

                (signed.hash, BytesJson::from(bytes.to_vec()))
            },
            EthPrivKeyPolicy::Trezor { .. } => {
                let derivation_path = derivation_path.or_mm_err(|| WithdrawError::FromAddressNotFound)?;
                let signed = self.sign_tx_with_trezor(derivation_path, &tx).await?;
                let bytes = rlp::encode(&signed);

                (signed.hash, BytesJson::from(bytes.to_vec()))
            },
            #[cfg(target_arch = "wasm32")]
            EthPrivKeyPolicy::Metamask(_) => {
                if !req.broadcast {
                    let error =
                        "Set 'broadcast' to generate, sign and broadcast a transaction with MetaMask".to_string();
                    return MmError::err(WithdrawError::BroadcastExpected(error));
                }

                let tx_to_send = TransactionRequest {
                    from: my_address,
                    to: Some(to_addr),
                    gas: Some(gas),
                    gas_price: Some(gas_price),
                    value: Some(eth_value),
                    data: Some(data.into()),
                    nonce: None,
                    ..TransactionRequest::default()
                };

                // Wait for 10 seconds for the transaction to appear on the RPC node.
                let wait_rpc_timeout = 10_000;
                let check_every = 1.;

                // Please note that this method may take a long time
                // due to `wallet_switchEthereumChain` and `eth_sendTransaction` requests.
                let tx_hash = coin.web3.eth().send_transaction(tx_to_send).await?;

                let signed_tx = coin
                    .wait_for_tx_appears_on_rpc(tx_hash, wait_rpc_timeout, check_every)
                    .await?;
                let tx_hex = signed_tx
                    .map(|tx| BytesJson::from(rlp::encode(&tx).to_vec()))
                    // Return an empty `tx_hex` if the transaction is still not appeared on the RPC node.
                    .unwrap_or_default();
                (tx_hash, tx_hex)
            },
        };

        let tx_hash_bytes = BytesJson::from(tx_hash.0.to_vec());
        let tx_hash_str = format!("{:02x}", tx_hash_bytes);

        let amount_decimal = u256_to_big_decimal(wei_amount, coin.decimals)?;
        let mut spent_by_me = amount_decimal.clone();
        let received_by_me = if to_addr == my_address {
            amount_decimal.clone()
        } else {
            0.into()
        };
        let fee_details = EthTxFeeDetails::new(gas, gas_price, fee_coin)?;
        if coin.coin_type == EthCoinType::Eth {
            spent_by_me += &fee_details.total_fee;
        }
        Ok(TransactionDetails {
            to: vec![checksum_address(&format!("{:#02x}", to_addr))],
            from: vec![checksum_address(&format!("{:#02x}", my_address))],
            total_amount: amount_decimal,
            my_balance_change: &received_by_me - &spent_by_me,
            spent_by_me,
            received_by_me,
            tx_hex,
            tx_hash: tx_hash_str,
            block_height: 0,
            fee_details: Some(fee_details.into()),
            coin: coin.ticker.clone(),
            internal_id: vec![].into(),
            timestamp: now_sec(),
            kmd_rewards: None,
            transaction_type: Default::default(),
            memo: None,
        })
    }
}

/// Eth withdraw version with user interaction support
pub struct InitEthWithdraw<'a> {
    ctx: MmArc,
    coin: EthCoin,
    task_handle: &'a WithdrawTaskHandle,
    req: WithdrawRequest,
}

#[async_trait]
impl<'a> EthWithdraw for InitEthWithdraw<'a> {
    fn coin(&self) -> &EthCoin { &self.coin }

    fn request(&self) -> &WithdrawRequest { &self.req }

    fn on_generating_transaction(&self) -> Result<(), MmError<WithdrawError>> {
        Ok(self
            .task_handle
            .update_in_progress_status(WithdrawInProgressStatus::GeneratingTransaction)?)
    }

    fn on_finishing(&self) -> Result<(), MmError<WithdrawError>> {
        Ok(self
            .task_handle
            .update_in_progress_status(WithdrawInProgressStatus::Finishing)?)
    }

    async fn sign_tx_with_trezor(
        &self,
        derivation_path: DerivationPath,
        unsigned_tx: &UnSignedEthTx,
    ) -> Result<SignedEthTx, MmError<WithdrawError>> {
        let coin = self.coin();
        let crypto_ctx = CryptoCtx::from_ctx(&self.ctx)?;
        let hw_ctx = crypto_ctx
            .hw_ctx()
            .or_mm_err(|| WithdrawError::HwError(HwRpcError::NoTrezorDeviceAvailable))?;
        let mut trezor_session = hw_ctx.trezor().await?;
        let chain_id = coin
            .chain_id
            .or_mm_err(|| WithdrawError::ChainIdRequired(String::from("chain_id is required for Trezor wallet")))?;
        let unverified_tx = trezor_session
            .sign_eth_tx(derivation_path, unsigned_tx, chain_id)
            .await?;
        Ok(SignedEthTx::new(unverified_tx).map_err(|err| WithdrawError::InternalError(err.to_string()))?)
    }
}

#[allow(clippy::result_large_err)]
impl<'a> InitEthWithdraw<'a> {
    pub fn new(
        ctx: MmArc,
        coin: EthCoin,
        req: WithdrawRequest,
        task_handle: &'a WithdrawTaskHandle,
    ) -> Result<InitEthWithdraw<'a>, MmError<WithdrawError>> {
        Ok(InitEthWithdraw {
            ctx,
            coin,
            task_handle,
            req,
        })
    }
}

/// Simple eth withdraw version without user interaction support
pub struct StandardEthWithdraw {
    coin: EthCoin,
    req: WithdrawRequest,
}

#[async_trait]
impl EthWithdraw for StandardEthWithdraw {
    fn coin(&self) -> &EthCoin { &self.coin }

    fn request(&self) -> &WithdrawRequest { &self.req }

    fn on_generating_transaction(&self) -> Result<(), MmError<WithdrawError>> { Ok(()) }

    fn on_finishing(&self) -> Result<(), MmError<WithdrawError>> { Ok(()) }

    async fn sign_tx_with_trezor(
        &self,
        _derivation_path: DerivationPath,
        _unsigned_tx: &UnSignedEthTx,
    ) -> Result<SignedEthTx, MmError<WithdrawError>> {
        async {
            Err(MmError::new(WithdrawError::UnsupportedError(String::from(
                "Trezor not supported for legacy RPC",
            ))))
        }
        .await
    }
}

#[allow(clippy::result_large_err)]
impl StandardEthWithdraw {
    pub fn new(coin: EthCoin, req: WithdrawRequest) -> Result<StandardEthWithdraw, MmError<WithdrawError>> {
        Ok(StandardEthWithdraw { coin, req })
    }
}
