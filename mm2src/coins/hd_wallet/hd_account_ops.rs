use super::HDWalletStorageError;
use crate::BalanceError;
use crypto::{Bip44Chain, DerivationPath};
use derive_more::Display;
use mm2_err_handle::prelude::*;

/// Currently, we suppose that ETH/ERC20/QRC20 don't have [`Bip44Chain::Internal`] addresses.
#[derive(Display)]
#[display(fmt = "Coin doesn't support the given BIP44 chain: {:?}", chain)]
pub struct InvalidBip44ChainError {
    pub chain: Bip44Chain,
}

#[derive(Display)]
pub enum AccountUpdatingError {
    AddressLimitReached { max_addresses_number: u32 },
    InvalidBip44Chain(InvalidBip44ChainError),
    WalletStorageError(HDWalletStorageError),
}

impl From<InvalidBip44ChainError> for AccountUpdatingError {
    fn from(e: InvalidBip44ChainError) -> Self { AccountUpdatingError::InvalidBip44Chain(e) }
}

impl From<HDWalletStorageError> for AccountUpdatingError {
    fn from(e: HDWalletStorageError) -> Self { AccountUpdatingError::WalletStorageError(e) }
}

impl From<AccountUpdatingError> for BalanceError {
    fn from(e: AccountUpdatingError) -> Self {
        let error = e.to_string();
        match e {
            AccountUpdatingError::AddressLimitReached { .. } | AccountUpdatingError::InvalidBip44Chain(_) => {
                // Account updating is expected to be called after `address_id` and `chain` validation.
                BalanceError::Internal(format!("Unexpected internal error: {}", error))
            },
            AccountUpdatingError::WalletStorageError(_) => BalanceError::WalletStorageError(error),
        }
    }
}

/// `HDAccountOps` Trait
///
/// Defines operations associated with an HD (Hierarchical Deterministic) account.
/// In the context of BIP-44 derivation paths, an HD account corresponds to the third level (`account'`)
/// in the structure `m / purpose' / coin_type' / account' / chain (or change) / address_index`.
/// This allows for segregating funds into different accounts under the same seed,
/// with each account having multiple chains (often representing internal and external addresses).
///
/// Implementors of this trait provide details about such HD account like its specific derivation path, known addresses, and its index.
pub trait HDAccountOps: Send + Sync {
    /// Returns the number of known addresses of this account.
    ///
    /// # Parameters
    ///
    /// * `chain`: The `Bip44Chain` representing the BIP44 chain of the addresses.
    ///
    /// # Returns
    ///
    /// A result containing a `u32` that represents the number of known addresses
    /// or an `InvalidBip44ChainError` if the coin doesn't support the given `chain`.
    fn known_addresses_number(&self, chain: Bip44Chain) -> MmResult<u32, InvalidBip44ChainError>;

    /// Fetches the derivation path associated with this account.
    ///
    /// # Returns
    ///
    /// A `DerivationPath` indicating the path used to derive this account.
    fn account_derivation_path(&self) -> DerivationPath;

    /// Retrieves the index of this account.
    ///
    /// The account index is used as part of the derivation path, following the pattern `m/purpose'/coin'/account'`.
    ///
    /// # Returns
    ///
    /// A `u32` value indicating the account's index.
    fn account_id(&self) -> u32;

    /// Checks if a specific address is activated (known) for this account at the present time.
    ///
    /// # Parameters
    ///
    /// * `chain`: The `Bip44Chain` representing the BIP44 chain of the address.
    /// * `address_id`: The id (or index) of the address in question.
    ///
    /// # Returns
    ///
    /// A result containing a `bool` indicating if the address is activated,
    /// or an `InvalidBip44ChainError` if the coin doesn't support the given `chain`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use your_crate::{HDAccountOps, Bip44Chain};
    /// # fn main() {
    /// let account: impl HDAccountOps = /* ... */;
    /// let is_activated = account.is_address_activated(Bip44Chain::External, 5).unwrap();
    /// println!("Is address 5 activated? {}", is_activated);
    /// # }
    /// ```
    fn is_address_activated(&self, chain: Bip44Chain, address_id: u32) -> MmResult<bool, InvalidBip44ChainError> {
        let is_activated = address_id < self.known_addresses_number(chain)?;
        Ok(is_activated)
    }
}
