use super::taker_swap::MaxTakerVolumeLessThanDust;
use super::{get_locked_amount, get_locked_amount_by_other_swaps};
use coins::{lp_coinfind_or_err, BalanceError, CoinFindError, MmCoinEnum, ProtocolSpecificBalance, TradeFee,
            TradePreimageError};
use common::log::debug;
use derive_more::Display;
use futures::compat::Future01CompatExt;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_number::{BigDecimal, MmNumber};
use uuid::Uuid;

pub type CheckBalanceResult<T> = Result<T, MmError<CheckBalanceError>>;
type CheckProtocolSpecificBalanceResult<T> = Result<T, MmError<CheckProtocolSpecificBalanceError>>;

/// Check the coin balance before the swap has started.
///
/// `swap_uuid` is used if our swap is running already and we should except this swap locked amount from the following calculations.
pub async fn check_my_coin_balance_for_swap(
    ctx: &MmArc,
    coin: &MmCoinEnum,
    swap_uuid: Option<&Uuid>,
    volume: MmNumber,
    mut trade_fee: TradeFee,
    taker_fee: Option<TakerFeeAdditionalInfo>,
) -> CheckBalanceResult<BigDecimal> {
    let ticker = coin.ticker();
    debug!("Check my_coin '{}' balance for swap", ticker);
    let balance: MmNumber = coin.my_spendable_balance().compat().await?.into();

    let locked = match swap_uuid {
        Some(u) => get_locked_amount_by_other_swaps(ctx, u, coin),
        None => get_locked_amount(ctx, coin),
    };

    let dex_fee = match taker_fee {
        Some(TakerFeeAdditionalInfo {
            dex_fee,
            fee_to_send_dex_fee,
        }) => {
            if fee_to_send_dex_fee.coin != trade_fee.coin {
                let err = format!(
                    "trade_fee {:?} and fee_to_send_dex_fee {:?} coins are expected to be the same",
                    trade_fee.coin, fee_to_send_dex_fee.coin
                );
                return MmError::err(CheckBalanceError::InternalError(err));
            }
            // increase `trade_fee` by the `fee_to_send_dex_fee`
            trade_fee.amount += fee_to_send_dex_fee.amount;
            dex_fee
        },
        None => MmNumber::from(0),
    };

    let total_trade_fee = if ticker == trade_fee.coin {
        trade_fee.amount
    } else {
        let base_coin_balance: MmNumber = coin.base_coin_balance().compat().await?.into();
        check_base_coin_balance_for_swap(ctx, &base_coin_balance, trade_fee, swap_uuid).await?;
        MmNumber::from(0)
    };

    debug!(
        "{} balance {:?}, locked {:?}, volume {:?}, fee {:?}, dex_fee {:?}",
        ticker,
        balance.to_fraction(),
        locked.locked_spendable.to_fraction(),
        volume.to_fraction(),
        total_trade_fee.to_fraction(),
        dex_fee.to_fraction()
    );

    let required = volume + total_trade_fee + dex_fee;
    let available = &balance - &locked.locked_spendable;

    if available < required {
        return MmError::err(CheckBalanceError::NotSufficientBalance {
            coin: ticker.to_owned(),
            available: available.to_decimal(),
            required: required.to_decimal(),
            locked_by_swaps: Some(locked.locked_spendable.to_decimal()),
        });
    }

    Ok(balance.into())
}

pub async fn check_other_coin_balance_for_swap(
    ctx: &MmArc,
    coin: &MmCoinEnum,
    swap_uuid: Option<&Uuid>,
    trade_fee: TradeFee,
    // Todo: change this name to be inline with protocol specific balance
    required_receivable_volume: MmNumber,
) -> CheckBalanceResult<()> {
    if trade_fee.paid_from_trading_vol {
        return Ok(());
    }
    let ticker = coin.ticker();
    debug!("Check other_coin '{}' balance for swap", ticker);
    let balance = coin.my_balance().compat().await?;
    let spendable_balance: MmNumber = balance.spendable.into();

    let locked = match swap_uuid {
        Some(u) => get_locked_amount_by_other_swaps(ctx, u, coin),
        None => get_locked_amount(ctx, coin),
    };

    if ticker == trade_fee.coin {
        let available = &spendable_balance - &locked.locked_spendable;
        let required = trade_fee.amount;
        debug!(
            "{} balance {:?}, locked {:?}, required {:?}",
            ticker,
            spendable_balance.to_fraction(),
            locked.locked_spendable.to_fraction(),
            required.to_fraction(),
        );
        if available < required {
            return MmError::err(CheckBalanceError::NotSufficientBalance {
                coin: ticker.to_owned(),
                available: available.to_decimal(),
                required: required.to_decimal(),
                locked_by_swaps: Some(locked.locked_spendable.to_decimal()),
            });
        }
    } else {
        let base_coin_balance: MmNumber = coin.base_coin_balance().compat().await?.into();
        check_base_coin_balance_for_swap(ctx, &base_coin_balance, trade_fee, swap_uuid).await?;
    }

    if let Some(protocol_specific_balance) = balance.protocol_specific_balance {
        check_other_coin_protocol_specific_balance_for_swap(
            ticker.to_string(),
            protocol_specific_balance,
            required_receivable_volume,
            // Todo: can this unwrap_or_default be removed?
            locked.locked_receivable.unwrap_or_default(),
        )?;
    }

    Ok(())
}

// Todo: add test cases in the test document for how to test inbound in ordermatching, maybe add unit tests
// Todo: check if this allow clippy can be removed at the end
#[allow(clippy::result_large_err)]
fn check_other_coin_protocol_specific_balance_for_swap(
    ticker: String,
    balance: ProtocolSpecificBalance,
    required_volume: MmNumber,
    locked: MmNumber,
) -> CheckProtocolSpecificBalanceResult<()> {
    match balance {
        // Todo: LSP on-demand liquidity case
        ProtocolSpecificBalance::Lightning(lightning_balance) => {
            let receivable_volume = MmNumber::from(lightning_balance.inbound) - locked.clone();
            let max_receivable_volume = MmNumber::from(lightning_balance.max_receivable_amount_per_payment);
            let receivable_volume = receivable_volume.min(max_receivable_volume);
            if receivable_volume < required_volume {
                return MmError::err(
                    CheckProtocolSpecificBalanceError::NotSufficientLightningInboundBalance {
                        coin: ticker,
                        available: receivable_volume.to_decimal(),
                        required: required_volume.to_decimal(),
                        locked_by_swaps: locked.to_decimal(),
                    },
                );
            }
            let min_receivable_volume = MmNumber::from(lightning_balance.min_receivable_amount_per_payment);
            if required_volume < min_receivable_volume {
                return MmError::err(CheckProtocolSpecificBalanceError::LightningInboundVolumeTooLow {
                    coin: ticker,
                    volume: required_volume.to_decimal(),
                    threshold: min_receivable_volume.to_decimal(),
                });
            }
        },
    }

    Ok(())
}

// Todo: will this be needed for LSP checks?
pub async fn check_base_coin_balance_for_swap(
    ctx: &MmArc,
    balance: &MmNumber,
    trade_fee: TradeFee,
    swap_uuid: Option<&Uuid>,
) -> CheckBalanceResult<()> {
    let ticker = trade_fee.coin.as_str();
    let coin = lp_coinfind_or_err(ctx, ticker).await?;
    let trade_fee_fraction = trade_fee.amount.to_fraction();
    debug!(
        "Check if the base coin '{}' has sufficient balance to pay the trade fee {:?}",
        ticker, trade_fee_fraction
    );

    let required = trade_fee.amount;
    let locked = match swap_uuid {
        Some(uuid) => get_locked_amount_by_other_swaps(ctx, uuid, &coin).locked_spendable,
        None => get_locked_amount(ctx, &coin).locked_spendable,
    };
    let available = balance - &locked;

    debug!(
        "{} balance {:?}, locked {:?}",
        ticker,
        balance.to_fraction(),
        locked.to_fraction()
    );
    if available < required {
        MmError::err(CheckBalanceError::NotSufficientBaseCoinBalance {
            coin: ticker.to_owned(),
            available: available.to_decimal(),
            required: required.to_decimal(),
            locked_by_swaps: Some(locked.to_decimal()),
        })
    } else {
        Ok(())
    }
}

pub struct TakerFeeAdditionalInfo {
    pub dex_fee: MmNumber,
    pub fee_to_send_dex_fee: TradeFee,
}

#[derive(Debug, Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum CheckBalanceError {
    #[display(fmt = "No such coin {}", coin)]
    NoSuchCoin { coin: String },
    #[display(
        fmt = "Not enough {} for swap: available {}, required at least {}, locked by swaps {:?}",
        coin,
        available,
        required,
        locked_by_swaps
    )]
    NotSufficientBalance {
        coin: String,
        available: BigDecimal,
        required: BigDecimal,
        locked_by_swaps: Option<BigDecimal>,
    },
    #[display(fmt = "Not enough protocol specific balance for swap: {}", _0)]
    NotSufficientProtocolSpecificBalance(CheckProtocolSpecificBalanceError),
    #[display(
        fmt = "Not enough base coin {} balance for swap: available {}, required at least {}, locked by swaps {:?}",
        coin,
        available,
        required,
        locked_by_swaps
    )]
    NotSufficientBaseCoinBalance {
        coin: String,
        available: BigDecimal,
        required: BigDecimal,
        locked_by_swaps: Option<BigDecimal>,
    },
    #[display(
        fmt = "The volume {} of the {} coin less than minimum transaction amount {}",
        volume,
        coin,
        threshold
    )]
    VolumeTooLow {
        coin: String,
        volume: BigDecimal,
        threshold: BigDecimal,
    },
    #[display(fmt = "Transport error: {}", _0)]
    Transport(String),
    #[display(fmt = "Internal error: {}", _0)]
    InternalError(String),
}

impl From<CoinFindError> for CheckBalanceError {
    fn from(e: CoinFindError) -> Self {
        match e {
            CoinFindError::NoSuchCoin { coin } => CheckBalanceError::NoSuchCoin { coin },
        }
    }
}

impl From<BalanceError> for CheckBalanceError {
    fn from(e: BalanceError) -> Self {
        match e {
            BalanceError::Transport(transport) | BalanceError::InvalidResponse(transport) => {
                CheckBalanceError::Transport(transport)
            },
            BalanceError::UnexpectedDerivationMethod(_) | BalanceError::WalletStorageError(_) => {
                CheckBalanceError::InternalError(e.to_string())
            },
            BalanceError::Internal(internal) => CheckBalanceError::InternalError(internal),
        }
    }
}

impl CheckBalanceError {
    pub fn not_sufficient_balance(&self) -> bool {
        matches!(
            self,
            CheckBalanceError::NotSufficientBalance { .. } | CheckBalanceError::NotSufficientBaseCoinBalance { .. }
        )
    }

    /// Construct [`CheckBalanceError`] from [`coins::TradePreimageError`] using the additional `ticker` argument.
    /// `ticker` is used to identify whether the `NotSufficientBalance` or `NotSufficientBaseCoinBalance` has occurred.
    pub fn from_trade_preimage_error(trade_preimage_err: TradePreimageError, ticker: &str) -> CheckBalanceError {
        match trade_preimage_err {
            TradePreimageError::NotSufficientBalance {
                coin,
                available,
                required,
            } => {
                if coin == ticker {
                    CheckBalanceError::NotSufficientBalance {
                        coin,
                        available,
                        locked_by_swaps: None,
                        required,
                    }
                } else {
                    CheckBalanceError::NotSufficientBaseCoinBalance {
                        coin,
                        available,
                        locked_by_swaps: None,
                        required,
                    }
                }
            },
            TradePreimageError::AmountIsTooSmall { amount, threshold } => CheckBalanceError::VolumeTooLow {
                coin: ticker.to_owned(),
                volume: amount,
                threshold,
            },
            TradePreimageError::Transport(transport) => CheckBalanceError::Transport(transport),
            TradePreimageError::InternalError(internal) => CheckBalanceError::InternalError(internal),
        }
    }

    pub fn from_max_taker_vol_error(
        max_vol_err: MaxTakerVolumeLessThanDust,
        coin: String,
        locked_by_swaps: BigDecimal,
    ) -> CheckBalanceError {
        CheckBalanceError::NotSufficientBalance {
            coin,
            available: max_vol_err.max_vol.to_decimal(),
            required: max_vol_err.min_tx_amount.to_decimal(),
            locked_by_swaps: Some(locked_by_swaps),
        }
    }
}

#[derive(Debug, Display, Serialize)]
pub enum CheckProtocolSpecificBalanceError {
    #[display(
        fmt = "Not enough {} inbound balance for swap: available {}, required at least {}, locked by swaps {}",
        coin,
        available,
        required,
        locked_by_swaps
    )]
    NotSufficientLightningInboundBalance {
        coin: String,
        available: BigDecimal,
        required: BigDecimal,
        locked_by_swaps: BigDecimal,
    },
    #[display(
        fmt = "The inbound volume {} of {} is less than minimum allowed inbound volume {}",
        volume,
        coin,
        threshold
    )]
    LightningInboundVolumeTooLow {
        coin: String,
        volume: BigDecimal,
        threshold: BigDecimal,
    },
}

impl From<CheckProtocolSpecificBalanceError> for CheckBalanceError {
    fn from(error: CheckProtocolSpecificBalanceError) -> Self {
        CheckBalanceError::NotSufficientProtocolSpecificBalance(error)
    }
}
