use std::fmt;
use std::str::FromStr;

use base58::{FromBase58, ToBase58};
use crypto::{checksum, ChecksumType};
use std::ops::Deref;
use {AddressHashEnum, DisplayLayout};

use crate::{address::detect_checksum, Error};

/// Struct for legacy address representation.
/// Note: LegacyAddress::from_str deserialization is added, which is used at least in the convertaddress rpc.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LegacyAddress {
    /// The prefix of the address.
    pub prefix: u8,
    /// T addr prefix, additional prefix used by Zcash and some forks
    pub t_addr_prefix: u8,
    /// Checksum type
    pub checksum_type: ChecksumType,
    /// Public key hash.
    pub hash: Vec<u8>,
}

pub struct LegacyAddressDisplayLayout(Vec<u8>);

impl Deref for LegacyAddressDisplayLayout {
    type Target = [u8];

    fn deref(&self) -> &Self::Target { &self.0 }
}

impl DisplayLayout for LegacyAddress {
    type Target = LegacyAddressDisplayLayout;

    fn layout(&self) -> Self::Target {
        let mut result = vec![];

        if self.t_addr_prefix > 0 {
            result.push(self.t_addr_prefix);
        }

        result.push(self.prefix);
        result.extend_from_slice(&self.hash.to_vec());
        let cs = checksum(&result, &self.checksum_type);
        result.extend_from_slice(&*cs);

        LegacyAddressDisplayLayout(result)
    }

    fn from_layout(data: &[u8]) -> Result<Self, Error>
    where
        Self: Sized,
    {
        match data.len() {
            25 => {
                let checksum_type = detect_checksum(&data[0..21], &data[21..])?;
                let hash = data[1..21].to_vec();

                let address = LegacyAddress {
                    t_addr_prefix: 0,
                    prefix: data[0],
                    checksum_type,
                    hash,
                };

                Ok(address)
            },
            26 => {
                let checksum_type = detect_checksum(&data[0..22], &data[22..])?;
                let hash = data[2..22].to_vec();

                let address = LegacyAddress {
                    t_addr_prefix: data[0],
                    prefix: data[1],
                    checksum_type,
                    hash,
                };

                Ok(address)
            },
            _ => Err(Error::InvalidAddress),
        }
    }
}

/// Converts legacy addresses from string
impl FromStr for LegacyAddress {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error>
    where
        Self: Sized,
    {
        let hex = s.from_base58().map_err(|_| Error::InvalidAddress)?;
        LegacyAddress::from_layout(&hex)
    }
}

impl From<&'static str> for LegacyAddress {
    fn from(s: &'static str) -> Self { s.parse().unwrap() } // TODO: dangerous unwrap?
}

impl fmt::Display for LegacyAddress {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result { self.layout().to_base58().fmt(fmt) }
}

impl LegacyAddress {
    pub fn new(hash: &AddressHashEnum, prefix: u8, t_addr_prefix: u8, checksum_type: ChecksumType) -> LegacyAddress {
        LegacyAddress {
            prefix,
            t_addr_prefix,
            checksum_type,
            hash: hash.to_vec(),
        }
    }
}
