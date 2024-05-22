// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
};

use anyhow::Context;
use ethers::core::rand::Rng;
use fendermint_crypto::{to_b64, PublicKey, SecretKey};
use fendermint_vm_actor_interface::{eam::EthAddress, init::builtin_actor_eth_addr, ipc};
use fendermint_vm_genesis::Genesis;
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;

use super::export;
use crate::{AccountId, AccountName, SubnetName};

pub struct DefaultDeployment {
    pub name: SubnetName,
    pub gateway: EthAddress,
    pub registry: EthAddress,
}

impl Display for DefaultDeployment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

impl DefaultDeployment {
    /// Deployment with the addresses that the Fendermint Genesis allocates.
    pub fn builtin(name: SubnetName) -> Self {
        Self {
            name,
            gateway: builtin_actor_eth_addr(ipc::GATEWAY_ACTOR_ID),
            registry: builtin_actor_eth_addr(ipc::SUBNETREGISTRY_ACTOR_ID),
        }
    }
}

pub struct DefaultGenesis {
    pub name: SubnetName,
    /// In-memory representation of the `genesis.json` file.
    pub genesis: Genesis,
    /// Path to the `genesis.json` file.
    pub path: PathBuf,
}

impl Display for DefaultGenesis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

pub struct DefaultSubnet {
    pub name: SubnetName,
    /// ID allocated to the subnet during creation.
    pub subnet_id: SubnetID,
}

impl Display for DefaultSubnet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

#[derive(PartialEq, Eq)]
pub struct DefaultAccount {
    name: AccountName,
    secret_key: SecretKey,
    public_key: PublicKey,
    /// Path to the directory where the keys are exported.
    path: PathBuf,
}

impl PartialOrd for DefaultAccount {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DefaultAccount {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl Debug for DefaultAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultAccount")
            .field("name", &self.name)
            .field("public_key", &self.public_key)
            .finish()
    }
}

impl Display for DefaultAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

impl DefaultAccount {
    pub fn account_id(&self) -> AccountId {
        self.name.0.id()
    }

    pub fn eth_addr(&self) -> EthAddress {
        EthAddress::from(self.public_key)
    }

    /// We assume that all accounts that interact with IPC are ethereum accounts.
    pub fn fvm_addr(&self) -> Address {
        self.eth_addr().into()
    }

    pub fn get_or_create<R: Rng>(
        rng: &mut R,
        root: impl AsRef<Path>,
        name: &AccountName,
    ) -> anyhow::Result<Self> {
        let dir = root.as_ref().join(name.path());
        let sk = dir.join("secret.hex");

        let (sk, is_new) = if sk.exists() {
            let sk = std::fs::read_to_string(sk).context("failed to read private key")?;
            let sk = hex::decode(sk).context("cannot decode hex private key")?;
            let sk = SecretKey::try_from(sk).context("failed to parse secret key")?;
            (sk, false)
        } else {
            let sk = SecretKey::random(rng);
            (sk, true)
        };

        let pk = sk.public_key();
        let acc = Self {
            name: name.clone(),
            secret_key: sk,
            public_key: pk,
            path: dir,
        };

        if is_new {
            acc.export()?;
        }

        Ok(acc)
    }

    /// Create (or overwrite) an account with a given secret key.
    pub fn create(
        root: impl AsRef<Path>,
        name: &AccountName,
        sk: SecretKey,
    ) -> anyhow::Result<Self> {
        let pk = sk.public_key();
        let dir = root.as_ref().join(name.path());
        let acc = Self {
            name: name.clone(),
            secret_key: sk,
            public_key: pk,
            path: dir,
        };
        acc.export()?;
        Ok(acc)
    }

    /// Write the keys to files.
    fn export(&self) -> anyhow::Result<()> {
        let sk = self.secret_key.serialize();
        let pk = self.public_key.serialize();

        export(&self.path, "secret", "b64", to_b64(sk.as_ref()))?;
        export(&self.path, "secret", "hex", hex::encode(sk))?;
        export(&self.path, "public", "b64", to_b64(pk.as_ref()))?;
        export(&self.path, "public", "hex", hex::encode(pk))?;
        export(&self.path, "eth-addr", "", format!("{:?}", self.eth_addr()))?;
        export(&self.path, "fvm-addr", "", self.fvm_addr().to_string())?;

        Ok(())
    }

    pub fn secret_key_path(&self) -> PathBuf {
        self.path.join("secret.b64")
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn secret_key(&self) -> &SecretKey {
        &self.secret_key
    }
}

#[cfg(test)]
mod tests {
    use ethers::core::rand::{rngs::StdRng, SeedableRng};
    use tempfile::TempDir;

    use crate::TestnetName;

    use super::DefaultAccount;

    #[test]
    fn test_account() {
        let mut rng = StdRng::from_entropy();
        let dir = TempDir::new().expect("temp dir created");
        let tn = TestnetName::new("account-test");
        let an1 = tn.account("account-1");
        let an2 = tn.account("account-2");

        let a1n = DefaultAccount::get_or_create(&mut rng, &dir, &an1)
            .expect("failed to create account-1");

        let a1e =
            DefaultAccount::get_or_create(&mut rng, &dir, &an1).expect("failed to get account-1");

        let a2n = DefaultAccount::get_or_create(&mut rng, &dir, &an2)
            .expect("failed to create account-2");

        assert_eq!(a1n, a1e, "should reload existing account");
        assert!(a1n != a2n, "should create new account per name");
    }
}
