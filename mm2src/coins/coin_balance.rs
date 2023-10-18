use crate::hd_wallet::{HDAccountAddressId, HDAccountOps, HDAddressId, HDAddressOps, HDCoinAddress, HDCoinHDAccount,
                       HDWalletCoinOps, HDWalletOps, HDXPubExtractor, NewAccountCreationError, NewAddressDerivingError};
use crate::{BalanceError, BalanceResult, CoinBalance, CoinWithDerivationMethod, DerivationMethod, MarketCoinOps};
use async_trait::async_trait;
use common::log::{debug, info};
use crypto::{Bip44Chain, RpcDerivationPath};
use futures::compat::Future01CompatExt;
use mm2_err_handle::prelude::*;
use mm2_number::BigDecimal;
#[cfg(test)] use mocktopus::macros::*;
use std::collections::HashMap;
use std::ops::Range;
use std::{fmt, iter};

pub type AddressIdRange = Range<u32>;
pub(crate) type HDBalanceAddress<T> = <<T as HDWalletBalanceOps>::HDAddressScanner as HDAddressBalanceScanner>::Address;

pub enum EnableCoinBalanceError {
    NewAddressDerivingError(NewAddressDerivingError),
    NewAccountCreationError(NewAccountCreationError),
    BalanceError(BalanceError),
}

impl From<NewAddressDerivingError> for EnableCoinBalanceError {
    fn from(e: NewAddressDerivingError) -> Self { EnableCoinBalanceError::NewAddressDerivingError(e) }
}

impl From<NewAccountCreationError> for EnableCoinBalanceError {
    fn from(e: NewAccountCreationError) -> Self { EnableCoinBalanceError::NewAccountCreationError(e) }
}

impl From<BalanceError> for EnableCoinBalanceError {
    fn from(e: BalanceError) -> Self { EnableCoinBalanceError::BalanceError(e) }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "wallet_type")]
pub enum CoinBalanceReport {
    Iguana(IguanaWalletBalance),
    HD(HDWalletBalance),
}

impl CoinBalanceReport {
    /// Returns a map where the key is address, and the value is the address's total balance [`CoinBalance::total`].
    pub fn to_addresses_total_balances(&self) -> HashMap<String, BigDecimal> {
        match self {
            CoinBalanceReport::Iguana(IguanaWalletBalance {
                ref address,
                ref balance,
            }) => iter::once((address.clone(), balance.get_total())).collect(),
            CoinBalanceReport::HD(HDWalletBalance { ref accounts }) => accounts
                .iter()
                .flat_map(|account_balance| {
                    account_balance
                        .addresses
                        .iter()
                        .map(|addr_balance| (addr_balance.address.clone(), addr_balance.balance.get_total()))
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IguanaWalletBalance {
    pub address: String,
    pub balance: CoinBalance,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDWalletBalance {
    pub accounts: Vec<HDAccountBalance>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDAccountBalance {
    pub account_index: u32,
    pub derivation_path: RpcDerivationPath,
    pub total_balance: CoinBalance,
    pub addresses: Vec<HDAddressBalance>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDAddressBalance {
    pub address: String,
    pub derivation_path: RpcDerivationPath,
    pub chain: Bip44Chain,
    pub balance: CoinBalance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnableCoinScanPolicy {
    /// Don't scan for new addresses.
    DoNotScan,
    /// Scan for new addresses if the coin HD wallet hasn't been enabled *only*.
    /// In other words, scan for new addresses if there were no HD accounts in the HD wallet storage.
    ScanIfNewWallet,
    /// Scan for new addresses even if the coin HD wallet has been enabled before.
    Scan,
}

impl Default for EnableCoinScanPolicy {
    fn default() -> Self { EnableCoinScanPolicy::ScanIfNewWallet }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EnabledCoinBalanceParams {
    #[serde(default)]
    pub scan_policy: EnableCoinScanPolicy,
    pub min_addresses_number: Option<u32>,
}

#[async_trait]
pub trait CoinBalanceReportOps {
    async fn coin_balance_report(&self) -> BalanceResult<CoinBalanceReport>;
}

#[async_trait]
impl<Coin> CoinBalanceReportOps for Coin
where
    Coin: CoinWithDerivationMethod + HDWalletBalanceOps + MarketCoinOps + Sync,
    HDCoinAddress<Coin>: fmt::Display + Sync,
{
    async fn coin_balance_report(&self) -> BalanceResult<CoinBalanceReport> {
        match self.derivation_method() {
            DerivationMethod::SingleAddress(my_address) => self
                .my_balance()
                .compat()
                .await
                .map(|balance| {
                    CoinBalanceReport::Iguana(IguanaWalletBalance {
                        address: my_address.to_string(),
                        balance,
                    })
                })
                .mm_err(BalanceError::from),
            DerivationMethod::HDWallet(hd_wallet) => self
                .all_accounts_balances(hd_wallet)
                .await
                .map(|accounts| CoinBalanceReport::HD(HDWalletBalance { accounts })),
        }
    }
}

#[async_trait]
pub trait EnableCoinBalanceOps {
    async fn enable_coin_balance<XPubExtractor>(
        &self,
        xpub_extractor: Option<XPubExtractor>,
        params: EnabledCoinBalanceParams,
        path_to_address: &HDAccountAddressId,
    ) -> MmResult<CoinBalanceReport, EnableCoinBalanceError>
    where
        XPubExtractor: HDXPubExtractor + Send;
}

#[async_trait]
impl<Coin> EnableCoinBalanceOps for Coin
where
    Coin: CoinWithDerivationMethod + HDWalletBalanceOps + MarketCoinOps + Sync,
    HDCoinAddress<Coin>: fmt::Display + Sync,
{
    async fn enable_coin_balance<XPubExtractor>(
        &self,
        xpub_extractor: Option<XPubExtractor>,
        params: EnabledCoinBalanceParams,
        path_to_address: &HDAccountAddressId,
    ) -> MmResult<CoinBalanceReport, EnableCoinBalanceError>
    where
        XPubExtractor: HDXPubExtractor + Send,
    {
        match self.derivation_method() {
            DerivationMethod::SingleAddress(my_address) => self
                .my_balance()
                .compat()
                .await
                .map(|balance| {
                    CoinBalanceReport::Iguana(IguanaWalletBalance {
                        address: my_address.to_string(),
                        balance,
                    })
                })
                .mm_err(EnableCoinBalanceError::from),
            DerivationMethod::HDWallet(hd_wallet) => self
                .enable_hd_wallet(hd_wallet, xpub_extractor, params, path_to_address)
                .await
                .map(CoinBalanceReport::HD),
        }
    }
}

#[async_trait]
pub trait HDWalletBalanceOps: HDWalletCoinOps {
    type HDAddressScanner: HDAddressBalanceScanner<Address = HDCoinAddress<Self>> + Sync;

    async fn produce_hd_address_scanner(&self) -> BalanceResult<Self::HDAddressScanner>;

    /// Requests balances of already known addresses, and if it's prescribed by [`EnableCoinParams::scan_policy`],
    /// scans for new addresses of every HD account by using [`HDWalletBalanceOps::scan_for_new_addresses`].
    /// This method is used on coin initialization to index working addresses and to return the wallet balance to the user.
    async fn enable_hd_wallet<XPubExtractor>(
        &self,
        hd_wallet: &Self::HDWallet,
        xpub_extractor: Option<XPubExtractor>,
        params: EnabledCoinBalanceParams,
        path_to_address: &HDAccountAddressId,
    ) -> MmResult<HDWalletBalance, EnableCoinBalanceError>
    where
        XPubExtractor: HDXPubExtractor + Send;

    /// Scans for the new addresses of the specified `hd_account` using the given `address_scanner`.
    /// Returns balances of the new addresses.
    async fn scan_for_new_addresses(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut HDCoinHDAccount<Self>,
        address_scanner: &Self::HDAddressScanner,
        gap_limit: u32,
    ) -> BalanceResult<Vec<HDAddressBalance>>;

    /// Requests balances of every activated HD account.
    async fn all_accounts_balances(&self, hd_wallet: &Self::HDWallet) -> BalanceResult<Vec<HDAccountBalance>> {
        let accounts = hd_wallet.get_accounts().await;

        let mut result = Vec::with_capacity(accounts.len());
        for (_account_id, hd_account) in accounts {
            let addresses = self.all_known_addresses_balances(&hd_account).await?;

            let total_balance = addresses.iter().fold(CoinBalance::default(), |total, addr_balance| {
                total + addr_balance.balance.clone()
            });
            let account_balance = HDAccountBalance {
                account_index: hd_account.account_id(),
                derivation_path: RpcDerivationPath(hd_account.account_derivation_path()),
                total_balance,
                addresses,
            };

            result.push(account_balance);
        }

        Ok(result)
    }

    /// Requests balances of every known addresses of the given `hd_account`.
    async fn all_known_addresses_balances(
        &self,
        hd_account: &HDCoinHDAccount<Self>,
    ) -> BalanceResult<Vec<HDAddressBalance>>;

    /// Requests balances of known addresses of the given `address_ids` addresses at the specified `chain`.
    async fn known_addresses_balances_with_ids<Ids>(
        &self,
        hd_account: &HDCoinHDAccount<Self>,
        chain: Bip44Chain,
        address_ids: Ids,
    ) -> BalanceResult<Vec<HDAddressBalance>>
    where
        HDCoinAddress<Self>: fmt::Display + Clone,
        Ids: Iterator<Item = u32> + Send,
    {
        let address_ids = address_ids.map(|address_id| HDAddressId { chain, address_id });

        // Derive HD addresses and split addresses and their derivation paths into two collections.
        let (addresses, der_paths): (Vec<_>, Vec<_>) = self
            .derive_addresses(hd_account, address_ids)
            .await?
            .into_iter()
            .map(|hd_address| (hd_address.address(), hd_address.derivation_path().clone()))
            .unzip();

        let balances = self
            .known_addresses_balances(addresses)
            .await?
            .into_iter()
            // [`HDWalletBalanceOps::known_addresses_balances`] returns pairs `(Address, CoinBalance)`
            // that are guaranteed to be in the same order in which they were requested.
            // So we can zip the derivation paths with the pairs `(Address, CoinBalance)`.
            .zip(der_paths)
            .map(|((address, balance), derivation_path)| HDAddressBalance {
                address: address.to_string(),
                derivation_path: RpcDerivationPath(derivation_path),
                chain,
                balance,
            })
            .collect();
        Ok(balances)
    }

    /// Requests balance of the given `address`.
    /// This function is expected to be more efficient than ['HDWalletBalanceOps::is_address_used'] in most cases
    /// since many of RPC clients allow us to request the address balance without the history.
    async fn known_address_balance(&self, address: &HDBalanceAddress<Self>) -> BalanceResult<CoinBalance>;

    /// Requests balances of the given `addresses`.
    /// The pairs `(Address, CoinBalance)` are guaranteed to be in the same order in which they were requested.
    async fn known_addresses_balances(
        &self,
        addresses: Vec<HDBalanceAddress<Self>>,
    ) -> BalanceResult<Vec<(HDBalanceAddress<Self>, CoinBalance)>>;

    /// Checks if the address has been used by the user by checking if the transaction history of the given `address` is not empty.
    /// Please note the function can return zero balance even if the address has been used before.
    async fn is_address_used(
        &self,
        address: &HDBalanceAddress<Self>,
        address_scanner: &Self::HDAddressScanner,
    ) -> BalanceResult<AddressBalanceStatus<CoinBalance>> {
        if !address_scanner.is_address_used(address).await? {
            return Ok(AddressBalanceStatus::NotUsed);
        }
        // Now we know that the address has been used.
        let balance = self.known_address_balance(address).await?;
        Ok(AddressBalanceStatus::Used(balance))
    }
}

// Todo: Move this trait to the `hd_wallet` module and most of this coin_balance file to the `hd_wallet` module while refactoring the code.
#[async_trait]
#[cfg_attr(test, mockable)]
pub trait HDAddressBalanceScanner {
    type Address: Send + Sync;

    async fn is_address_used(&self, address: &Self::Address) -> BalanceResult<bool>;
}

pub enum AddressBalanceStatus<Balance> {
    Used(Balance),
    NotUsed,
}

pub mod common_impl {
    use super::*;
    use crate::hd_wallet::{create_new_account, ExtractExtendedPubkey, HDAccountOps, HDAccountStorageOps, HDAddressOps,
                           HDWalletOps};
    use crypto::Secp256k1ExtendedPublicKey;

    pub(crate) async fn enable_hd_account<Coin>(
        coin: &Coin,
        hd_wallet: &Coin::HDWallet,
        hd_account: &mut HDCoinHDAccount<Coin>,
        chain: Bip44Chain,
        address_scanner: &Coin::HDAddressScanner,
        scan_new_addresses: bool,
        min_addresses_number: Option<u32>,
    ) -> MmResult<HDAccountBalance, EnableCoinBalanceError>
    where
        Coin: HDWalletBalanceOps + MarketCoinOps + Sync,
        HDCoinAddress<Coin>: fmt::Display,
    {
        let gap_limit = hd_wallet.gap_limit();
        let mut addresses = coin.all_known_addresses_balances(hd_account).await?;
        if scan_new_addresses {
            addresses.extend(
                coin.scan_for_new_addresses(hd_wallet, hd_account, address_scanner, gap_limit)
                    .await?,
            );
        }

        if let Some(min_addresses_number) = min_addresses_number {
            gen_new_addresses(coin, hd_wallet, hd_account, chain, &mut addresses, min_addresses_number).await?
        }

        let total_balance = addresses.iter().fold(CoinBalance::default(), |total, addr_balance| {
            total + addr_balance.balance.clone()
        });
        let account_balance = HDAccountBalance {
            account_index: hd_account.account_id(),
            derivation_path: RpcDerivationPath(hd_account.account_derivation_path()),
            total_balance,
            addresses,
        };

        Ok(account_balance)
    }

    pub(crate) async fn enable_hd_wallet<Coin, XPubExtractor>(
        coin: &Coin,
        hd_wallet: &Coin::HDWallet,
        xpub_extractor: Option<XPubExtractor>,
        params: EnabledCoinBalanceParams,
        path_to_address: &HDAccountAddressId,
    ) -> MmResult<HDWalletBalance, EnableCoinBalanceError>
    where
        Coin: ExtractExtendedPubkey<ExtendedPublicKey = Secp256k1ExtendedPublicKey>
            + HDWalletBalanceOps
            + MarketCoinOps
            + Sync,
        HDCoinAddress<Coin>: fmt::Display,
        XPubExtractor: HDXPubExtractor + Send,
        HDCoinHDAccount<Coin>: HDAccountStorageOps,
    {
        let mut accounts = hd_wallet.get_accounts_mut().await;
        let address_scanner = coin.produce_hd_address_scanner().await?;

        let mut result = HDWalletBalance {
            accounts: Vec::with_capacity(accounts.len() + 1),
        };

        if accounts.get(&path_to_address.account_id).is_none() {
            // Is seems that we couldn't find any HD account from the HD wallet storage.
            drop(accounts);
            info!(
                "{} HD wallet hasn't been enabled before. Create default HD account",
                coin.ticker()
            );

            // Create new HD account.
            let mut new_account =
                create_new_account(coin, hd_wallet, xpub_extractor, Some(path_to_address.account_id)).await?;
            let scan_new_addresses = matches!(
                params.scan_policy,
                EnableCoinScanPolicy::ScanIfNewWallet | EnableCoinScanPolicy::Scan
            );

            let account_balance = enable_hd_account(
                coin,
                hd_wallet,
                &mut new_account,
                path_to_address.chain,
                &address_scanner,
                scan_new_addresses,
                params.min_addresses_number.max(Some(path_to_address.address_id + 1)),
            )
            .await?;
            // Todo: The enabled address should be indicated in the response.
            result.accounts.push(account_balance);
            return Ok(result);
        }

        debug!(
            "{} HD accounts were found on {} coin activation",
            accounts.len(),
            coin.ticker()
        );
        let scan_new_addresses = matches!(params.scan_policy, EnableCoinScanPolicy::Scan);
        for (account_id, hd_account) in accounts.iter_mut() {
            let min_addresses_number = if *account_id == path_to_address.account_id {
                // The account for the enabled address is already indexed.
                // But in case the address index is larger than the number of derived addresses,
                // we need to derive new addresses to make sure that the enabled address is indexed.
                params.min_addresses_number.max(Some(path_to_address.address_id + 1))
            } else {
                params.min_addresses_number
            };
            let account_balance = enable_hd_account(
                coin,
                hd_wallet,
                hd_account,
                path_to_address.chain,
                &address_scanner,
                scan_new_addresses,
                min_addresses_number,
            )
            .await?;
            result.accounts.push(account_balance);
        }

        Ok(result)
    }

    /// Generate new address so that the total number of `result_addresses` addresses is at least `min_addresses_number`.
    async fn gen_new_addresses<Coin>(
        coin: &Coin,
        hd_wallet: &Coin::HDWallet,
        hd_account: &mut HDCoinHDAccount<Coin>,
        chain: Bip44Chain,
        result_addresses: &mut Vec<HDAddressBalance>,
        min_addresses_number: u32,
    ) -> MmResult<(), EnableCoinBalanceError>
    where
        Coin: HDWalletBalanceOps + MarketCoinOps + Sync,
        HDCoinAddress<Coin>: fmt::Display,
    {
        let max_addresses_number = hd_account.address_limit();
        if min_addresses_number >= max_addresses_number {
            return MmError::err(EnableCoinBalanceError::NewAddressDerivingError(
                NewAddressDerivingError::AddressLimitReached { max_addresses_number },
            ));
        }

        let min_addresses_number = min_addresses_number as usize;
        let actual_addresses_number = result_addresses.len();
        if actual_addresses_number >= min_addresses_number {
            // There are more or equal to the `min_addresses_number` known addresses already.
            return Ok(());
        }

        let to_generate = min_addresses_number - actual_addresses_number;
        let ticker = coin.ticker();
        let account_id = hd_account.account_id();
        info!("Generate '{to_generate}' addresses: ticker={ticker} account_id={account_id}, chain={chain:?}");

        let mut new_addresses = Vec::with_capacity(to_generate);
        let mut addresses_to_request = Vec::with_capacity(to_generate);
        for _ in 0..to_generate {
            let hd_address = coin.generate_new_address(hd_wallet, hd_account, chain).await?;

            new_addresses.push(HDAddressBalance {
                address: hd_address.address().to_string(),
                derivation_path: RpcDerivationPath(hd_address.derivation_path().clone()),
                chain,
                balance: CoinBalance::default(),
            });
            addresses_to_request.push(hd_address.address().clone());
        }

        let to_extend = coin
            .known_addresses_balances(addresses_to_request)
            .await?
            .into_iter()
            // The balances are guaranteed to be in the same order as they were requests.
            .zip(new_addresses)
            .map(|((_address, balance), mut address_info)| {
                address_info.balance = balance;
                address_info
            });

        result_addresses.extend(to_extend);
        Ok(())
    }
}
