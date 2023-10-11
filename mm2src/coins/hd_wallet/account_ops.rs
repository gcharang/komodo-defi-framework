use super::{HDAddressOps, HDAddressesCache, InvalidBip44ChainError};
use crypto::{Bip44Chain, DerivationPath, Secp256k1ExtendedPublicKey, StandardHDPathToAccount};
use mm2_err_handle::prelude::*;

/// `HDAccountOps` Trait
///
/// Defines operations associated with an HD (Hierarchical Deterministic) account.
/// In the context of BIP-44 derivation paths, an HD account corresponds to the third level (`account'`)
/// in the structure `m / purpose' / coin_type' / account' / chain (or change) / address_index`.
/// This allows for segregating funds into different accounts under the same seed,
/// with each account having multiple chains (often representing internal and external addresses).
///
/// Implementors of this trait provide details about such HD account like its specific derivation path, known addresses, and its index.
pub trait HDAccountOps {
    type HDAddress: HDAddressOps + Clone + Send;

    /// Creates a new `HDAccountOps` instance.
    ///
    /// # Parameters
    ///
    /// * `account_id`: The index of the account.
    /// * `account_extended_pubkey`: The extended public key associated with this account.
    /// * `account_derivation_path`: The derivation path from the master key to this account.
    ///
    /// # Returns
    ///
    /// A new `HDAccountOps` instance.
    fn new(
        account_id: u32,
        account_extended_pubkey: Secp256k1ExtendedPublicKey,
        account_derivation_path: StandardHDPathToAccount,
    ) -> Self;

    /// Provides the limit on the number of addresses that can be added to an account.
    ///
    /// # Returns
    ///
    /// A `u32` value indicating the maximum number of addresses.
    /// The default is given by `DEFAULT_ADDRESS_LIMIT`.
    fn address_limit(&self) -> u32;

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

    /// Sets the number of known addresses of this account.
    ///
    /// # Parameters
    ///
    /// * `chain`: The `Bip44Chain` representing the BIP44 chain of the addresses.
    /// * `new_known_addresses_number`: The new number of known addresses.
    fn set_known_addresses_number(&mut self, chain: Bip44Chain, new_known_addresses_number: u32);

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
    fn is_address_activated(&self, chain: Bip44Chain, address_id: u32) -> MmResult<bool, InvalidBip44ChainError>;

    /// Fetches the derived addresses from cache.
    ///
    /// # Returns
    ///
    /// A `HDAddressesCache` containing the derived addresses.
    fn derived_addresses(&self) -> &HDAddressesCache<Self::HDAddress>;

    /// Fetches the extended public key associated with this account.
    ///
    /// # Returns
    ///
    /// A `Secp256k1ExtendedPublicKey` type representing the extended public key.
    fn extended_pubkey(&self) -> &Secp256k1ExtendedPublicKey;
}
