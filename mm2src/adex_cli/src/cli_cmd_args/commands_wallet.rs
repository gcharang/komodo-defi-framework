use anyhow::{anyhow, bail, Result};
use clap::{Args, Subcommand, ValueEnum};
use hex::FromHexError;
use rpc::v1::types::Bytes as BytesJson;
use std::mem::take;
use std::str::FromStr;
use std::{f64, u64};

use common::log::error;
use mm2_number::BigDecimal;
use mm2_rpc::data::legacy::BalanceRequest;
use mm2_rpc::data::version2::GetRawTransactionRequest;

use crate::rpc_data::{Bip44Chain, HDAccountAddressId, SendRawTransactionRequest, WithdrawFee, WithdrawFrom,
                      WithdrawRequest};
use crate::{error_anyhow, error_bail};

#[derive(Subcommand)]
pub(crate) enum WalletCommands {
    #[command(visible_alias = "balance", about = "Get coin balance")]
    MyBalance(MyBalanceArgs),
    #[command(
        about = "Generates, signs, and returns a transaction that transfers the amount of coin to \
                 the address indicated in the to argument"
    )]
    Withdraw(WithdrawArgs),
    #[command(
        visible_aliases = ["send-raw", "send"],
        about = "Broadcasts the transaction to the network of selected coin"
    )]
    SendRawTransaction(SendRawTransactionArgs),
    #[command(
        visible_aliases = ["get-raw", "raw-tx", "get"],
        about = "Returns the full signed raw transaction hex for any transaction that is confirmed \
                 or within the mempool"
    )]
    GetRawTransaction(GetRawTransactionArgs),
}

#[derive(Args)]
pub(crate) struct MyBalanceArgs {
    #[arg(name = "COIN", help = "Coin to get balance")]
    coin: String,
}

impl From<&mut MyBalanceArgs> for BalanceRequest {
    fn from(value: &mut MyBalanceArgs) -> Self {
        BalanceRequest {
            coin: take(&mut value.coin),
        }
    }
}

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
    #[arg(
        long,
        short,
        default_value_t = false,
        visible_alias = "bare",
        help = "Whether to output only tx_hash"
    )]
    pub(crate) bare_output: bool,
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

#[derive(Args)]
pub(crate) struct WithdrawArgs {
    #[arg(help = "Coin the user desires to withdraw")]
    coin: String,
    #[arg(help = "Address the user desires to withdraw to")]
    to: String,
    #[command(flatten)]
    amount: WithdrawAmountArg,
    #[arg(
        long,
        short,
        value_parser = parse_withdraw_fee,
        help = "Transaction fee [possible-values: <utxo-fixed:amount|utxo-per-kbyte:amount|eth:gas_price:gas|qrc:gas_limit:gas_price|cosmos:gas_limit:gas_price>]"
    )]
    fee: Option<WithdrawFeeArg>,
    #[command(flatten)]
    from: WithdrawFromArgs,
    #[arg(
        long,
        short,
        default_value_t = false,
        visible_alias = "bare",
        help = "Whether to output only tx_hex"
    )]
    pub(crate) bare_output: bool,
}

#[derive(Args)]
#[group(required = false, multiple = true)]
struct WithdrawFromArgs {
    #[arg(
        long,
        help = "Derivation path to determine the source of the derived value in more detail"
    )]
    derivation_path: Option<String>,
    #[arg(
        long,
        help = "AddressId hd_account_id to determine the source of the derived value in more detail"
    )]
    hd_account_id: Option<u32>,
    #[arg(
        long,
        value_enum,
        help = "AddressId chain to determine the source of the derived value in more detail"
    )]
    hd_account_chain: Option<Bip44ChainArg>,
    #[arg(long, help = "AddressId to determine the source of the derived value in more detail")]
    hd_account_address_id: Option<u32>,
}

#[derive(Clone, Debug, ValueEnum)]
enum Bip44ChainArg {
    External,
    Internal,
}

#[derive(Args)]
#[group(required = false, multiple = false)]
pub(crate) struct WithdrawAmountArg {
    #[arg(
        long,
        short = 'M',
        group = "withdraw-amount",
        default_value_t = false,
        help = "Withdraw the maximum available amount"
    )]
    pub(crate) max: bool,
    #[arg(long, short, group = "withdraw-amount", help = "Amount the user desires to withdraw")]
    pub(crate) amount: Option<BigDecimal>,
}

#[derive(Clone)]
pub(crate) enum WithdrawFeeArg {
    UtxoFixed { amount: BigDecimal },
    UtxoPerKbyte { amount: BigDecimal },
    EthGas { gas_price: BigDecimal, gas: u64 },
    Qrc20Gas { gas_limit: u64, gas_price: u64 },
    CosmosGas { gas_limit: u64, gas_price: f64 },
}

fn parse_withdraw_fee(value: &str) -> Result<WithdrawFeeArg> {
    #[derive(Clone, ValueEnum)]
    pub(crate) enum WithdrawFeeTag {
        UtxoFixed,
        UtxoPerKbyte,
        Eth,
        Qrc,
        Cosmos,
    }
    let mut parts = value.split(':');
    match parts.next() {
        Some(tag) => match WithdrawFeeTag::from_str(tag, false) {
            Ok(WithdrawFeeTag::UtxoFixed) => {
                let Some(amount) = parts.next() else {
                    error_bail!("Failed to parse utxo-fixed fee, no amount literal");
                };
                let amount = BigDecimal::from_str(amount)
                    .map_err(|error| error_anyhow!("Failed to parse amount from: {}, error: {}", amount, error))?;
                Ok(WithdrawFeeArg::UtxoFixed { amount })
            },
            Ok(WithdrawFeeTag::UtxoPerKbyte) => {
                let Some(amount) = parts.next() else {
                    error_bail!("Failed to parse utxo-per-kbyte fee, no amount literal");
                };
                let amount = BigDecimal::from_str(amount)
                    .map_err(|error| error_anyhow!("Failed to parse amount from: {}, error: {}", amount, error))?;
                Ok(WithdrawFeeArg::UtxoPerKbyte { amount })
            },
            Ok(WithdrawFeeTag::Eth) => {
                let Some(gas_price) = parts.next() else {
                    error_bail!("Failed to parse eth fee, no gas_price literal");
                };
                let gas_price = BigDecimal::from_str(gas_price).map_err(|error| {
                    error_anyhow!("Failed to parse gas_price from: {}, error: {}", gas_price, error)
                })?;
                let Some(gas) = parts.next() else {
                    error_bail!("Failed to parse eth fee, no gas literal");
                };
                let gas = u64::from_str(gas)
                    .map_err(|error| error_anyhow!("Failed to parse gas from: {}, error: {}", gas, error))?;

                Ok(WithdrawFeeArg::EthGas { gas_price, gas })
            },
            Ok(WithdrawFeeTag::Qrc) => {
                let Some(gas_limit) = parts.next() else {
                    error_bail!("Failed to parse qrc fee, no gas_limit literal");
                };
                let gas_limit = u64::from_str(gas_limit).map_err(|error| {
                    error_anyhow!("Failed to parse gas_limit from: {}, error: {}", gas_limit, error)
                })?;

                let Some(gas_price) = parts.next() else {
                    error_bail!("Failed to parse qrc fee, no gas_price literal");
                };
                let gas_price = u64::from_str(gas_price).map_err(|error| {
                    error_anyhow!("Failed to parse gas_price from: {}, error: {}", gas_price, error)
                })?;
                Ok(WithdrawFeeArg::Qrc20Gas { gas_limit, gas_price })
            },
            Ok(WithdrawFeeTag::Cosmos) => {
                let Some(gas_limit) = parts.next() else {
                    error_bail!("Failed to parse cosomos fee, no gas_limit literal");
                };
                let gas_limit = u64::from_str(gas_limit).map_err(|error| {
                    error_anyhow!("Failed to parse gas_limit from: {}, error: {}", gas_limit, error)
                })?;

                let Some(gas_price) = parts.next() else {
                    error_bail!("Failed to parse cosmos fee, no gas_price literal");
                };
                let gas_price = f64::from_str(gas_price).map_err(|error| {
                    error_anyhow!("Failed to parse gas_price from: {}, error: {}", gas_price, error)
                })?;
                Ok(WithdrawFeeArg::CosmosGas { gas_limit, gas_price })
            },
            Err(error) => {
                error_bail!("Failed to parse fee_tag: {}", error)
            },
        },
        None => {
            error_bail!("Failed to parse withdraw_fee: tag literal has not been found");
        },
    }
}

impl From<&mut WithdrawArgs> for WithdrawRequest {
    fn from(value: &mut WithdrawArgs) -> Self {
        WithdrawRequest {
            coin: take(&mut value.coin),
            from: None,
            to: take(&mut value.to),
            amount: value.amount.amount.take().unwrap_or_default(),
            max: value.amount.max,
            fee: value.fee.as_mut().map(WithdrawFee::from),
        }
    }
}

impl From<&mut WithdrawFeeArg> for WithdrawFee {
    fn from(value: &mut WithdrawFeeArg) -> Self {
        match value {
            WithdrawFeeArg::UtxoFixed { amount } => WithdrawFee::UtxoFixed { amount: take(amount) },
            WithdrawFeeArg::UtxoPerKbyte { amount } => WithdrawFee::UtxoPerKbyte { amount: take(amount) },
            WithdrawFeeArg::EthGas { gas_price, gas } => WithdrawFee::EthGas {
                gas_price: take(gas_price),
                gas: take(gas),
            },
            WithdrawFeeArg::Qrc20Gas { gas_limit, gas_price } => WithdrawFee::Qrc20Gas {
                gas_limit: take(gas_limit),
                gas_price: take(gas_price),
            },
            WithdrawFeeArg::CosmosGas { gas_limit, gas_price } => WithdrawFee::CosmosGas {
                gas_limit: take(gas_limit),
                gas_price: take(gas_price),
            },
        }
    }
}

impl TryFrom<&mut WithdrawFromArgs> for WithdrawFrom {
    type Error = anyhow::Error;

    fn try_from(value: &mut WithdrawFromArgs) -> std::result::Result<Self, Self::Error> {
        if let Some(derivation_path) = value.derivation_path.take() {
            return Ok(WithdrawFrom::DerivationPath { derivation_path });
        };
        let account_id = value.hd_account_id.take();
        let chain = value.hd_account_chain.take();
        let address_id = value.hd_account_address_id.take();
        if let (Some(account_id), Some(chain), Some(address_id)) = (account_id, chain.clone(), address_id) {
            let chain = match chain {
                Bip44ChainArg::Internal => Bip44Chain::Internal,
                Bip44ChainArg::External => Bip44Chain::External,
            };
            return Ok(WithdrawFrom::AddressId(HDAccountAddressId {
                account_id,
                chain,
                address_id,
            }));
        };
        error_bail!(
            "Failed to get withdraw_from due to params incompatibility: account_id: {:?}, chain: {:?}, address_id: {:?}",
            account_id,
            chain,
            address_id
        )
    }
}

#[derive(Args)]
pub(crate) struct GetRawTransactionArgs {
    #[arg(long, short, help = "Coin the user desires to request for the transaction")]
    pub(crate) coin: String,
    #[arg(
        long,
        value_parser=parse_bytes,
        visible_alias = "hash",
        short = 'h',
        help = "Hash of the transaction"
    )]
    pub(crate) tx_hash: BytesJson,
    #[arg(
        long,
        short,
        default_value_t = false,
        visible_alias = "bare",
        help = "Whether to output only tx_hex"
    )]
    pub(crate) bare_output: bool,
}

impl From<&mut GetRawTransactionArgs> for GetRawTransactionRequest {
    fn from(value: &mut GetRawTransactionArgs) -> Self {
        GetRawTransactionRequest {
            coin: take(&mut value.coin),
            tx_hash: hex::encode(value.tx_hash.as_slice()),
        }
    }
}
