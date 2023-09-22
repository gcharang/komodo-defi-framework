use crate::hd_confirm_address::HDConfirmAddress;
use crate::hd_pubkey::HDXPubExtractor;
use crate::hd_wallet::{inner_impl, AccountUpdatingError, AddressDerivingError, AddressDerivingResult, HDAccountMut,
                       HDAccountOps, HDAddress, HDWalletOps, NewAccountCreatingError, NewAddressDeriveConfirmError,
                       NewAddressDerivingError};
use async_trait::async_trait;
use crypto::Bip44Chain;
use itertools::Itertools;
use mm2_err_handle::mm_error::{MmError, MmResult};

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct HDAddressId {
    pub chain: Bip44Chain,
    pub address_id: u32,
}

/// `HDWalletCoinOps` defines operations that coins should support to have HD wallet functionalities.
/// This trait outlines fundamental operations like address derivation, account creation, and more.
#[async_trait]
pub trait HDWalletCoinOps {
    /// The type representing an address in the coin's context.
    type Address: Clone + Send + Sync;
    /// The type representing a public key.
    type Pubkey: Send;
    /// The type representing the HD Wallet operations associated with this coin.
    type HDWallet: HDWalletOps<Address = Self::Address, HDAccount = Self::HDAccount>;
    /// The type representing an HD account for this coin.
    type HDAccount: HDAccountOps;

    /// Derives a single HD address for a given account, chain, and address identifier.
    ///
    /// # Parameters
    /// - `hd_account`: The HD account from which the address will be derived.
    /// - `chain`: The Bip44 chain identifier.
    /// - `address_id`: The unique address identifier.
    ///
    /// # Returns
    /// A result containing the derived `HDAddress<Self::Address, Self::Pubkey>` instance or an error.
    async fn derive_address(
        &self,
        hd_account: &Self::HDAccount,
        chain: Bip44Chain,
        address_id: u32,
    ) -> AddressDerivingResult<HDAddress<Self::Address, Self::Pubkey>> {
        self.derive_addresses(hd_account, std::iter::once(HDAddressId { chain, address_id }))
            .await?
            .into_iter()
            .exactly_one()
            // Unfortunately, we can't use [`MapToMmResult::map_to_mm`] due to unsatisfied trait bounds,
            // and it's easier to use [`Result::map_err`] instead of adding more trait bounds to this method.
            .map_err(|e| MmError::new(AddressDerivingError::Internal(e.to_string())))
    }

    /// Derives a set of HD addresses for a coin using the specified HD account and address identifiers.
    ///
    /// # Parameters
    /// - `hd_account`: The HD account associated with the addresses.
    /// - `address_ids`: An iterator of `HDAddressId` items specifying which addresses to derive.
    ///
    /// # Returns
    /// A result containing a vector of derived `HDAddress<Self::Address, Self::Pubkey>` instances or an error.
    async fn derive_addresses<Ids>(
        &self,
        hd_account: &Self::HDAccount,
        address_ids: Ids,
    ) -> AddressDerivingResult<Vec<HDAddress<Self::Address, Self::Pubkey>>>
    where
        Ids: Iterator<Item = HDAddressId> + Send;

    /// Derives known HD addresses for a specific account and chain.
    /// Essentially, this retrieves addresses that have been interacted with in the past.
    ///
    /// # Parameters
    /// - `hd_account`: The HD account from which to derive known addresses.
    /// - `chain`: The Bip44 chain identifier.
    ///
    /// # Returns
    /// A result containing a vector of previously generated `HDAddress<Self::Address, Self::Pubkey>` instances or an error.
    async fn derive_known_addresses(
        &self,
        hd_account: &Self::HDAccount,
        chain: Bip44Chain,
    ) -> AddressDerivingResult<Vec<HDAddress<Self::Address, Self::Pubkey>>> {
        let known_addresses_number = hd_account.known_addresses_number(chain)?;
        let address_ids = (0..known_addresses_number)
            .into_iter()
            .map(|address_id| HDAddressId { chain, address_id });
        self.derive_addresses(hd_account, address_ids).await
    }

    /// Generates a new address for a coin and updates the corresponding number of used `hd_account` addresses.
    ///
    /// # Parameters
    /// - `hd_wallet`: The specified HD wallet from which the address will be derived.
    /// - `hd_account`: The mutable HD account.
    /// - `chain`: The Bip44 chain identifier.
    ///
    /// # Returns
    /// A result containing the generated `HDAddress<Self::Address, Self::Pubkey>` instance or an error.
    async fn generate_new_address(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut Self::HDAccount,
        chain: Bip44Chain,
    ) -> MmResult<HDAddress<Self::Address, Self::Pubkey>, NewAddressDerivingError> {
        let inner_impl::NewAddress {
            address,
            new_known_addresses_number,
        } = inner_impl::generate_new_address_immutable(self, hd_wallet, hd_account, chain).await?;

        self.set_known_addresses_number(hd_wallet, hd_account, chain, new_known_addresses_number)
            .await?;
        Ok(address)
    }

    /// Generates a new address with an added confirmation step.
    /// This method prompts the user to verify if the derived address matches
    /// the hardware wallet display, ensuring security and accuracy when
    /// dealing with hardware wallets.
    ///
    /// # Parameters
    /// - `hd_wallet`: The specified HD wallet.
    /// - `hd_account`: The mutable HD account.
    /// - `chain`: The Bip44 chain identifier.
    /// - `confirm_address`: Address confirmation method.
    ///
    /// # Returns
    /// A result containing the confirmed `HDAddress<Self::Address, Self::Pubkey>` instance or an error.
    async fn generate_and_confirm_new_address<ConfirmAddress>(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut Self::HDAccount,
        chain: Bip44Chain,
        confirm_address: &ConfirmAddress,
    ) -> MmResult<HDAddress<Self::Address, Self::Pubkey>, NewAddressDeriveConfirmError>
    where
        ConfirmAddress: HDConfirmAddress;

    /// Creates and registers a new HD account for a specific wallet.
    ///
    /// # Parameters
    /// - `hd_wallet`: The specified HD wallet.
    /// - `xpub_extractor`: Optional method for extracting the extended public key.
    ///   This is especially useful when dealing with hardware wallets. It can
    ///   allow for the extraction of the extended public key directly from the
    ///   wallet when needed.
    /// - `account_id`: Optional account identifier.
    ///
    /// # Returns
    /// A result containing a mutable reference to the created `Self::HDAccount` or an error.
    async fn create_new_account<'a, XPubExtractor>(
        &self,
        hd_wallet: &'a Self::HDWallet,
        xpub_extractor: Option<XPubExtractor>,
        account_id: Option<u32>,
    ) -> MmResult<HDAccountMut<'a, Self::HDAccount>, NewAccountCreatingError>
    where
        XPubExtractor: HDXPubExtractor + Send;

    /// Updates the count of known addresses for a specified HD account and chain.
    /// This is useful for tracking the number of created addresses.
    ///
    /// # Parameters
    /// - `hd_wallet`: The specified HD wallet.
    /// - `hd_account`: The mutable HD account to be updated.
    /// - `chain`: The Bip44 chain identifier.
    /// - `new_known_addresses_number`: The new count of known addresses.
    ///
    /// # Returns
    /// A result indicating success or an error.
    async fn set_known_addresses_number(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut Self::HDAccount,
        chain: Bip44Chain,
        new_known_addresses_number: u32,
    ) -> MmResult<(), AccountUpdatingError>;
}
