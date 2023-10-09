use std::{convert::{TryFrom, TryInto},
          fmt, u8};

pub struct ConstPrefixes<'a> {
    p2pkh: &'a [u8],
    p2sh: &'a [u8],
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, Default)]
pub struct AddressPrefixes {
    data: Vec<u8>,
}

impl TryFrom<&[u8]> for AddressPrefixes {
    type Error = ();

    fn try_from(prefixes: &[u8]) -> Result<Self, Self::Error> {
        if !prefixes.is_empty() && prefixes.len() <= 2 {
            Ok(Self {
                data: prefixes.to_vec(),
            })
        } else {
            Err(())
        }
    }
}

impl From<[u8; 1]> for AddressPrefixes {
    fn from(prefixes: [u8; 1]) -> Self {
        Self {
            data: prefixes.to_vec(),
        }
    }
}

impl From<[u8; 2]> for AddressPrefixes {
    fn from(prefixes: [u8; 2]) -> Self {
        Self {
            data: prefixes.to_vec(),
        }
    }
}

impl fmt::Display for AddressPrefixes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        for i in 0..self.data.len() {
            write!(f, "{}", self.data[i])?;
            if i < self.data.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, "]")?;
        Ok(())
    }
}

impl AddressPrefixes {
    /// Get as vec of u8
    pub fn to_vec(&self) -> Vec<u8> { self.data.to_vec() }

    /// Get if prefixes size is 1, for use in cash_address
    pub fn get_size_1_prefix(&self) -> u8 {
        if self.data.len() == 1 {
            self.data[0]
        } else {
            0 // maybe assert should be here as it is not supposed to have other prefixes size for cahs_address
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkAddressPrefixes {
    pub p2pkh: AddressPrefixes,
    pub p2sh: AddressPrefixes,
}

impl TryFrom<ConstPrefixes<'_>> for NetworkAddressPrefixes {
    type Error = ();

    fn try_from(const_prefixes: ConstPrefixes) -> Result<Self, Self::Error> {
        Ok(Self {
            p2pkh: const_prefixes.p2pkh.try_into()?,
            p2sh: const_prefixes.p2sh.try_into()?,
        })
    }
}

impl fmt::Display for NetworkAddressPrefixes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{")?;
        write!(f, "{}", self.p2pkh)?;
        write!(f, "{}", self.p2sh)?;

        write!(f, "}}")?;
        Ok(())
    }
}

pub mod prefixes {
    use super::ConstPrefixes;

    pub const KMD_P2PKH: [u8; 1] = [60];
    pub const KMD_P2SH: [u8; 1] = [85];
    pub const KMD_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &KMD_P2PKH,
        p2sh: &KMD_P2SH,
    };

    pub const BTC_P2PKH: [u8; 1] = [0];
    pub const BTC_P2SH: [u8; 1] = [5];
    pub const BTC_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &BTC_P2PKH,
        p2sh: &BTC_P2SH,
    };

    pub const T_BTC_P2PKH: [u8; 1] = [111];
    pub const T_BTC_P2SH: [u8; 1] = [196];
    pub const T_BTC_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &T_BTC_P2PKH,
        p2sh: &T_BTC_P2SH,
    };

    pub const BCH_P2PKH: [u8; 1] = [0];
    pub const BCH_P2SH: [u8; 1] = [5];
    pub const BCH_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &BCH_P2PKH,
        p2sh: &BCH_P2SH,
    };

    pub const QRC20_P2PKH: [u8; 1] = [120];
    pub const QRC20_P2SH: [u8; 1] = [50];
    pub const QRC20_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &QRC20_P2PKH,
        p2sh: &QRC20_P2SH,
    };

    pub const QTUM_P2PKH: [u8; 1] = [58];
    pub const QTUM_P2SH: [u8; 1] = [50];
    pub const QTUM_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &QTUM_P2PKH,
        p2sh: &QTUM_P2SH,
    };

    pub const GRS_P2PKH: [u8; 1] = [36];
    pub const GRS_P2SH: [u8; 1] = [5];
    pub const GRS_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &GRS_P2PKH,
        p2sh: &GRS_P2SH,
    };

    pub const SYS_P2PKH: [u8; 1] = [63];
    pub const SYS_P2SH: [u8; 1] = [5];
    pub const SYS_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &SYS_P2PKH,
        p2sh: &SYS_P2SH,
    };

    pub const ZCASH_P2PKH: [u8; 2] = [28, 184];
    pub const ZCASH_P2SH: [u8; 2] = [28, 189];
    pub const ZCASH_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &ZCASH_P2PKH,
        p2sh: &ZCASH_P2SH,
    };

    pub const T_ZCASH_P2PKH: [u8; 2] = [29, 37];
    pub const T_ZCASH_P2SH: [u8; 2] = [28, 186];
    pub const T_ZCASH_PREFIXES: ConstPrefixes<'static> = ConstPrefixes {
        p2pkh: &T_ZCASH_P2PKH,
        p2sh: &T_ZCASH_P2SH,
    };
}
