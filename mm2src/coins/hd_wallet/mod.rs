use crate::{BalanceError, WithdrawError};
use async_trait::async_trait;
use crypto::{Bip32DerPathError, Bip32DerPathOps, Bip32Error, Bip44Chain, ChildNumber, DerivationPath, HwError,
             StandardHDPath, StandardHDPathError, StandardHDPathToCoin};
use derive_more::Display;
use futures::lock::{MappedMutexGuard as AsyncMappedMutexGuard, Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use mm2_err_handle::prelude::*;
use rpc_task::RpcTaskError;
use serde::Serialize;
use std::collections::BTreeMap;

mod account_ops;
pub use account_ops::{AccountUpdatingError, HDAccountOps, InvalidBip44ChainError};

mod coin_ops;
pub use coin_ops::{HDAddressId, HDWalletCoinOps};

mod confirm_address;
#[cfg(test)]
pub(crate) use confirm_address::for_tests::MockableConfirmAddress;
pub(crate) use confirm_address::{ConfirmAddressStatus, RpcTaskConfirmAddress};
pub use confirm_address::{HDConfirmAddress, HDConfirmAddressError};

mod pubkey;
pub use pubkey::{ExtractExtendedPubkey, HDExtractPubkeyError, HDXPubExtractor, RpcTaskXPubExtractor};

mod storage;
#[cfg(target_arch = "wasm32")]
pub(crate) use storage::HDWalletDb;
#[cfg(test)] pub(crate) use storage::HDWalletMockStorage;
pub use storage::{HDAccountStorageItem, HDWalletCoinStorage, HDWalletCoinWithStorageOps, HDWalletId,
                  HDWalletStorageError};
pub(crate) use storage::{HDWalletStorageInternalOps, HDWalletStorageResult};

pub(crate) type HDAccountsMap<HDAccount> = BTreeMap<u32, HDAccount>;
pub(crate) type HDAccountsMutex<HDAccount> = AsyncMutex<HDAccountsMap<HDAccount>>;
pub(crate) type HDAccountsMut<'a, HDAccount> = AsyncMutexGuard<'a, HDAccountsMap<HDAccount>>;
pub(crate) type HDAccountMut<'a, HDAccount> = AsyncMappedMutexGuard<'a, HDAccountsMap<HDAccount>, HDAccount>;

pub(crate) type AddressDerivingResult<T> = MmResult<T, AddressDerivingError>;

const DEFAULT_ADDRESS_LIMIT: u32 = ChildNumber::HARDENED_FLAG;
const DEFAULT_ACCOUNT_LIMIT: u32 = ChildNumber::HARDENED_FLAG;
const DEFAULT_RECEIVER_CHAIN: Bip44Chain = Bip44Chain::External;

#[derive(Debug, Display)]
pub enum AddressDerivingError {
    #[display(fmt = "Coin doesn't support the given BIP44 chain: {:?}", chain)]
    InvalidBip44Chain {
        chain: Bip44Chain,
    },
    #[display(fmt = "BIP32 address deriving error: {}", _0)]
    Bip32Error(Bip32Error),
    Internal(String),
}

impl From<InvalidBip44ChainError> for AddressDerivingError {
    fn from(e: InvalidBip44ChainError) -> Self { AddressDerivingError::InvalidBip44Chain { chain: e.chain } }
}

impl From<Bip32Error> for AddressDerivingError {
    fn from(e: Bip32Error) -> Self { AddressDerivingError::Bip32Error(e) }
}

impl From<AddressDerivingError> for BalanceError {
    fn from(e: AddressDerivingError) -> Self { BalanceError::Internal(e.to_string()) }
}

impl From<AddressDerivingError> for WithdrawError {
    fn from(e: AddressDerivingError) -> Self {
        match e {
            AddressDerivingError::InvalidBip44Chain { .. } | AddressDerivingError::Bip32Error(_) => {
                WithdrawError::UnexpectedFromAddress(e.to_string())
            },
            AddressDerivingError::Internal(internal) => WithdrawError::InternalError(internal),
        }
    }
}

#[derive(Display)]
pub enum NewAddressDerivingError {
    #[display(fmt = "Addresses limit reached. Max number of addresses: {}", max_addresses_number)]
    AddressLimitReached { max_addresses_number: u32 },
    #[display(fmt = "Coin doesn't support the given BIP44 chain: {:?}", chain)]
    InvalidBip44Chain { chain: Bip44Chain },
    #[display(fmt = "BIP32 address deriving error: {}", _0)]
    Bip32Error(Bip32Error),
    #[display(fmt = "Wallet storage error: {}", _0)]
    WalletStorageError(HDWalletStorageError),
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
}

impl From<Bip32Error> for NewAddressDerivingError {
    fn from(e: Bip32Error) -> Self { NewAddressDerivingError::Bip32Error(e) }
}

impl From<AddressDerivingError> for NewAddressDerivingError {
    fn from(e: AddressDerivingError) -> Self {
        match e {
            AddressDerivingError::InvalidBip44Chain { chain } => NewAddressDerivingError::InvalidBip44Chain { chain },
            AddressDerivingError::Bip32Error(bip32) => NewAddressDerivingError::Bip32Error(bip32),
            AddressDerivingError::Internal(internal) => NewAddressDerivingError::Internal(internal),
        }
    }
}

impl From<InvalidBip44ChainError> for NewAddressDerivingError {
    fn from(e: InvalidBip44ChainError) -> Self { NewAddressDerivingError::InvalidBip44Chain { chain: e.chain } }
}

impl From<AccountUpdatingError> for NewAddressDerivingError {
    fn from(e: AccountUpdatingError) -> Self {
        match e {
            AccountUpdatingError::AddressLimitReached { max_addresses_number } => {
                NewAddressDerivingError::AddressLimitReached { max_addresses_number }
            },
            AccountUpdatingError::InvalidBip44Chain(e) => NewAddressDerivingError::from(e),
            AccountUpdatingError::WalletStorageError(storage) => NewAddressDerivingError::WalletStorageError(storage),
        }
    }
}

pub enum NewAddressDeriveConfirmError {
    DeriveError(NewAddressDerivingError),
    ConfirmError(HDConfirmAddressError),
}

impl From<HDConfirmAddressError> for NewAddressDeriveConfirmError {
    fn from(e: HDConfirmAddressError) -> Self { NewAddressDeriveConfirmError::ConfirmError(e) }
}

impl From<NewAddressDerivingError> for NewAddressDeriveConfirmError {
    fn from(e: NewAddressDerivingError) -> Self { NewAddressDeriveConfirmError::DeriveError(e) }
}

impl From<AccountUpdatingError> for NewAddressDeriveConfirmError {
    fn from(e: AccountUpdatingError) -> Self {
        NewAddressDeriveConfirmError::DeriveError(NewAddressDerivingError::from(e))
    }
}

impl From<InvalidBip44ChainError> for NewAddressDeriveConfirmError {
    fn from(e: InvalidBip44ChainError) -> Self {
        NewAddressDeriveConfirmError::DeriveError(NewAddressDerivingError::from(e))
    }
}

#[derive(Display)]
pub enum NewAccountCreatingError {
    #[display(fmt = "Hardware Wallet context is not initialized")]
    HwContextNotInitialized,
    #[display(fmt = "HD wallet is unavailable")]
    HDWalletUnavailable,
    #[display(
        fmt = "Coin doesn't support Trezor hardware wallet. Please consider adding the 'trezor_coin' field to the coins config"
    )]
    CoinDoesntSupportTrezor,
    RpcTaskError(RpcTaskError),
    HardwareWalletError(HwError),
    #[display(fmt = "Accounts limit reached. Max number of accounts: {}", max_accounts_number)]
    AccountLimitReached {
        max_accounts_number: u32,
    },
    #[display(fmt = "Error saving HD account to storage: {}", _0)]
    ErrorSavingAccountToStorage(String),
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
}

impl From<Bip32DerPathError> for NewAccountCreatingError {
    fn from(e: Bip32DerPathError) -> Self {
        NewAccountCreatingError::Internal(StandardHDPathError::from(e).to_string())
    }
}

impl From<HDWalletStorageError> for NewAccountCreatingError {
    fn from(e: HDWalletStorageError) -> Self {
        match e {
            HDWalletStorageError::ErrorSaving(e) | HDWalletStorageError::ErrorSerializing(e) => {
                NewAccountCreatingError::ErrorSavingAccountToStorage(e)
            },
            HDWalletStorageError::HDWalletUnavailable => NewAccountCreatingError::HDWalletUnavailable,
            HDWalletStorageError::Internal(internal) => NewAccountCreatingError::Internal(internal),
            other => NewAccountCreatingError::Internal(other.to_string()),
        }
    }
}

#[derive(Clone)]
pub struct HDAddress<Address, Pubkey> {
    pub address: Address,
    pub pubkey: Pubkey,
    pub derivation_path: DerivationPath,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HDAccountAddressId {
    pub account_id: u32,
    pub chain: Bip44Chain,
    pub address_id: u32,
}

impl Default for HDAccountAddressId {
    fn default() -> Self {
        HDAccountAddressId {
            account_id: 0,
            chain: Bip44Chain::External,
            address_id: 0,
        }
    }
}

impl From<StandardHDPath> for HDAccountAddressId {
    fn from(der_path: StandardHDPath) -> Self {
        HDAccountAddressId {
            account_id: der_path.account_id(),
            chain: der_path.chain(),
            address_id: der_path.address_id(),
        }
    }
}

impl HDAccountAddressId {
    pub fn to_derivation_path(
        &self,
        path_to_coin: &StandardHDPathToCoin,
    ) -> Result<DerivationPath, MmError<Bip32Error>> {
        let mut account_der_path = path_to_coin.to_derivation_path();
        account_der_path.push(ChildNumber::new(self.account_id, true)?);
        account_der_path.push(self.chain.to_child_number());
        account_der_path.push(ChildNumber::new(self.address_id, false)?);
        drop_mutability!(account_der_path);

        Ok(account_der_path)
    }
}

/// `HDWalletOps`: Operations that should be implemented for Structs that represent HD wallets.
#[async_trait]
pub trait HDWalletOps: Send + Sync {
    /// The specific address type used by this wallet.
    type Address;
    /// The HD account operations associated with this wallet.
    type HDAccount: HDAccountOps + Clone + Send;

    /// Returns the coin type associated with this HD Wallet.
    ///
    /// This method can be implemented to fetch the coin type as specified in the wallet's BIP44 derivation path.
    /// For example, in the derivation path `m/44'/0'/0'/0`, the coin type would be `0` (representing Bitcoin).
    ///
    /// # Returns
    ///
    /// A `u32` value representing the coin type from the wallet's derivation path.
    fn coin_type(&self) -> u32;

    /// Fetches the gap limit associated with this HD Wallet.
    ///
    /// # Returns
    ///
    /// A `u32` value that specifies the gap limit.
    fn gap_limit(&self) -> u32;

    /// Provides the limit on the number of addresses that can be added to an account.
    ///
    /// # Returns
    ///
    /// A `u32` value indicating the maximum number of addresses.
    /// The default is given by `DEFAULT_ADDRESS_LIMIT`.
    fn address_limit(&self) -> u32 { DEFAULT_ADDRESS_LIMIT }

    /// Specifies the limit on the number of accounts that can be added to the wallet.
    ///
    /// # Returns
    ///
    /// A `u32` value indicating the maximum number of accounts.
    /// The default is set by `DEFAULT_ACCOUNT_LIMIT`.
    fn account_limit(&self) -> u32 { DEFAULT_ACCOUNT_LIMIT }

    /// Specifies the default BIP44 chain for receiver addresses.
    ///
    /// # Returns
    ///
    /// A `Bip44Chain` value.
    /// The default is set by `DEFAULT_RECEIVER_CHAIN`.
    fn default_receiver_chain(&self) -> Bip44Chain { DEFAULT_RECEIVER_CHAIN }

    /// Provides a mutex that guards the HD accounts.
    ///
    /// # Returns
    ///
    /// A reference to the accounts mutex.
    fn get_accounts_mutex(&self) -> &HDAccountsMutex<Self::HDAccount>;

    /// Fetches an account based on its ID. This method will return `None` if the account is not activated.
    ///
    /// # Parameters
    ///
    /// - `account_id`: The ID of the desired account.
    ///
    /// # Returns
    ///
    /// An `Option<Self::HDAccount>` which contains the account if found.
    async fn get_account(&self, account_id: u32) -> Option<Self::HDAccount> {
        let accounts = self.get_accounts_mutex().lock().await;
        accounts.get(&account_id).cloned()
    }

    /// Similar to `get_account`, but provides a mutable reference.
    ///
    /// # Parameters
    ///
    /// - `account_id`: The ID of the desired account.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the account if found.
    async fn get_account_mut(&self, account_id: u32) -> Option<HDAccountMut<'_, Self::HDAccount>> {
        let accounts = self.get_accounts_mutex().lock().await;
        if !accounts.contains_key(&account_id) {
            return None;
        }

        Some(AsyncMutexGuard::map(accounts, |accounts| {
            accounts
                .get_mut(&account_id)
                .expect("getting an element should never fail due to the checks above")
        }))
    }

    /// Gathers all the activated accounts.
    ///
    /// # Returns
    ///
    /// A map containing all the currently activated HD accounts.
    async fn get_accounts(&self) -> HDAccountsMap<Self::HDAccount> { self.get_accounts_mutex().lock().await.clone() }

    /// Similar to `get_accounts`, but provides a mutable reference to the accounts.
    ///
    /// # Returns
    ///
    /// A mutable reference to the map of all activated HD accounts.
    async fn get_accounts_mut(&self) -> HDAccountsMut<'_, Self::HDAccount> { self.get_accounts_mutex().lock().await }

    /// Attempts to remove an account only if it's the last in the set.
    ///
    /// # Parameters
    ///
    /// - `account_id`: The ID of the account to be considered for removal.
    ///
    /// # Returns
    ///
    /// An `Option` containing the removed HD account if it was indeed the last one, otherwise `None`.
    async fn remove_account_if_last(&self, account_id: u32) -> Option<Self::HDAccount> {
        let mut x = self.get_accounts_mutex().lock().await;
        // `BTreeMap::last_entry` is still unstable.
        let (last_account_id, _) = x.iter().last()?;
        if *last_account_id == account_id {
            x.remove(&account_id)
        } else {
            None
        }
    }

    /// Returns an address that's currently enabled for single-address operations, such as swaps.
    ///
    /// # Returns
    ///
    /// An `Option` containing the enabled address if available.
    async fn get_enabled_address(&self) -> Option<Self::Address>;
}

pub(crate) mod inner_impl {
    use super::*;
    use coin_ops::HDWalletCoinOps;

    pub struct NewAddress<Address, Pubkey> {
        pub address: HDAddress<Address, Pubkey>,
        pub new_known_addresses_number: u32,
    }

    /// Generates a new address without updating a corresponding number of used `hd_account` addresses.
    pub async fn generate_new_address_immutable<Coin>(
        coin: &Coin,
        hd_wallet: &Coin::HDWallet,
        hd_account: &Coin::HDAccount,
        chain: Bip44Chain,
    ) -> MmResult<NewAddress<Coin::Address, Coin::Pubkey>, NewAddressDerivingError>
    where
        Coin: HDWalletCoinOps + ?Sized + Sync,
    {
        let known_addresses_number = hd_account.known_addresses_number(chain)?;
        // Address IDs start from 0, so the `known_addresses_number = last_known_address_id + 1`.
        let new_address_id = known_addresses_number;
        let max_addresses_number = hd_wallet.address_limit();
        if new_address_id >= max_addresses_number {
            return MmError::err(NewAddressDerivingError::AddressLimitReached { max_addresses_number });
        }
        let address = coin.derive_address(hd_account, chain, new_address_id).await?;
        Ok(NewAddress {
            address,
            new_known_addresses_number: known_addresses_number + 1,
        })
    }
}
