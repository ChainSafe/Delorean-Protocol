// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use std::str::FromStr;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

mod evm;
mod fvm;

#[cfg(feature = "with-ethers")]
pub use crate::evm::{random_eth_key_info, EthKeyAddress};
pub use crate::evm::{
    KeyInfo as EvmKeyInfo, KeyStore as EvmKeyStore, PersistentKeyInfo, PersistentKeyStore,
    DEFAULT_KEYSTORE_NAME,
};
pub use crate::fvm::*;

/// WalletType determines the kind of keys and wallets
/// supported in the keystore
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "network_type")]
pub enum WalletType {
    Evm,
    Fvm,
}

impl FromStr for WalletType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "evm" => Self::Evm,
            "fvm" => Self::Fvm,
            _ => return Err(anyhow!("invalid wallet type")),
        })
    }
}
