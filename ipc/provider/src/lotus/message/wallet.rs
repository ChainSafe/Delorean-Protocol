// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use anyhow::anyhow;
use fvm_shared::crypto::signature::SignatureType;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Display, EnumString, AsRefStr)]
pub enum WalletKeyType {
    #[strum(serialize = "bls", ascii_case_insensitive)]
    BLS,
    #[strum(serialize = "secp256k1", ascii_case_insensitive)]
    Secp256k1,
    #[strum(serialize = "secp256k1-ledger", ascii_case_insensitive)]
    Secp256k1Ledger,
}

impl TryFrom<WalletKeyType> for SignatureType {
    type Error = anyhow::Error;

    fn try_from(value: WalletKeyType) -> Result<Self, Self::Error> {
        match value {
            WalletKeyType::BLS => Ok(SignatureType::BLS),
            WalletKeyType::Secp256k1 => Ok(SignatureType::Secp256k1),
            WalletKeyType::Secp256k1Ledger => Err(anyhow!("type not supported")),
        }
    }
}

impl TryFrom<SignatureType> for WalletKeyType {
    type Error = anyhow::Error;

    fn try_from(value: SignatureType) -> Result<Self, Self::Error> {
        match value {
            SignatureType::BLS => Ok(WalletKeyType::BLS),
            SignatureType::Secp256k1 => Ok(WalletKeyType::Secp256k1),
        }
    }
}

pub type WalletListResponse = Vec<String>;

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::lotus::message::wallet::WalletKeyType;

    #[test]
    fn test_key_types() {
        let t = WalletKeyType::Secp256k1;
        assert_eq!(t.as_ref(), "secp256k1");

        let t = WalletKeyType::from_str(t.as_ref()).unwrap();
        assert_eq!(t, WalletKeyType::Secp256k1);
    }
}
