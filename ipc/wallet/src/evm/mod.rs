// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

//! Ethereum wallet key store.

mod memory;
mod persistent;

use anyhow::Result;
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use zeroize::Zeroize;

#[cfg(feature = "with-ethers")]
use std::str::FromStr;

pub use crate::evm::persistent::{PersistentKeyInfo, PersistentKeyStore};

pub const DEFAULT_KEYSTORE_NAME: &str = "evm_keystore.json";

/// The key store trait for different evm key store
pub trait KeyStore {
    /// The type of the key that is stored
    type Key: Clone + Eq + Hash + TryFrom<KeyInfo>;

    /// Get the key info by address string
    fn get(&self, addr: &Self::Key) -> Result<Option<KeyInfo>>;
    /// List all addresses in the key store
    fn list(&self) -> Result<Vec<Self::Key>>;
    /// Put a new info to the addr
    fn put(&mut self, info: KeyInfo) -> Result<Self::Key>;
    /// Remove address from the key store
    fn remove(&mut self, addr: &Self::Key) -> Result<()>;
    /// Set default wallet
    fn set_default(&mut self, addr: &Self::Key) -> Result<()>;
    /// Get default wallet
    fn get_default(&mut self) -> Result<Option<Self::Key>>;
}

/// The struct that contains evm private key info
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyInfo {
    private_key: Vec<u8>,
}

impl KeyInfo {
    pub fn new(private_key: Vec<u8>) -> Self {
        Self { private_key }
    }
}

impl KeyInfo {
    pub fn private_key(&self) -> &[u8] {
        &self.private_key
    }
}

impl Drop for KeyInfo {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

#[cfg(feature = "with-ethers")]
impl TryFrom<KeyInfo> for ethers::types::Address {
    type Error = anyhow::Error;

    fn try_from(value: KeyInfo) -> std::result::Result<Self, Self::Error> {
        use ethers::signers::Signer;
        let key = ethers::signers::Wallet::from_bytes(&value.private_key)?;
        Ok(key.address())
    }
}

#[cfg(feature = "with-ethers")]
impl FromStr for EthKeyAddress {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = ethers::types::Address::from_str(s)?;
        Ok(EthKeyAddress { inner })
    }
}

#[cfg(feature = "with-ethers")]
pub fn random_eth_key_info() -> KeyInfo {
    let key = ethers::core::k256::SecretKey::random(&mut rand::thread_rng());
    KeyInfo::new(key.to_bytes().to_vec())
}

#[cfg(feature = "with-ethers")]
#[derive(Debug, Clone, Eq, Hash, PartialEq, Default)]
pub struct EthKeyAddress {
    inner: ethers::types::Address,
}

#[cfg(feature = "with-ethers")]
impl From<ethers::types::Address> for EthKeyAddress {
    fn from(inner: ethers::types::Address) -> Self {
        EthKeyAddress { inner }
    }
}

#[cfg(feature = "with-ethers")]
impl TryFrom<EthKeyAddress> for fvm_shared::address::Address {
    type Error = hex::FromHexError;

    fn try_from(value: EthKeyAddress) -> std::result::Result<Self, Self::Error> {
        Ok(fvm_shared::address::Address::from(
            &ipc_types::EthAddress::from_str(&value.to_string())?,
        ))
    }
}

#[cfg(feature = "with-ethers")]
impl From<EthKeyAddress> for ethers::types::Address {
    fn from(val: EthKeyAddress) -> Self {
        val.inner
    }
}

#[cfg(feature = "with-ethers")]
impl Display for EthKeyAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self == &Self::default() {
            write!(f, "default-key")
        } else {
            write!(f, "{:?}", self.inner)
        }
    }
}

#[cfg(feature = "with-ethers")]
impl TryFrom<KeyInfo> for EthKeyAddress {
    type Error = anyhow::Error;

    fn try_from(value: KeyInfo) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            inner: ethers::types::Address::try_from(value)?,
        })
    }
}
