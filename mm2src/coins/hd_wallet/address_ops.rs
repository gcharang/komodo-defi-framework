use bip32::DerivationPath;
use std::fmt::Display;

pub trait HDAddressOps {
    type Address: Clone + Display + Send + Sync;
    type Pubkey;

    fn address(&self) -> Self::Address;
    fn pubkey(&self) -> &Self::Pubkey;
    fn derivation_path(&self) -> &DerivationPath;
}
