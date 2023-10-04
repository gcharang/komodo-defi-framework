use super::{HDConfirmAddressError, HDWalletStorageError};
use bip32::Error as Bip32Error;
use crypto::{Bip32DerPathError, Bip44Chain, HwError, StandardHDPathError};
use rpc_task::RpcTaskError;

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
pub enum NewAccountCreationError {
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

impl From<Bip32DerPathError> for NewAccountCreationError {
    fn from(e: Bip32DerPathError) -> Self {
        NewAccountCreationError::Internal(StandardHDPathError::from(e).to_string())
    }
}

impl From<HDWalletStorageError> for NewAccountCreationError {
    fn from(e: HDWalletStorageError) -> Self {
        match e {
            HDWalletStorageError::ErrorSaving(e) | HDWalletStorageError::ErrorSerializing(e) => {
                NewAccountCreationError::ErrorSavingAccountToStorage(e)
            },
            HDWalletStorageError::HDWalletUnavailable => NewAccountCreationError::HDWalletUnavailable,
            HDWalletStorageError::Internal(internal) => NewAccountCreationError::Internal(internal),
            other => NewAccountCreationError::Internal(other.to_string()),
        }
    }
}

// Todo: Need to change implementation to support no change/internal addresses for Ethereum
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
