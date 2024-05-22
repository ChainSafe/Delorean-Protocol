// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

// See https://github.com/cometbft/cometbft/blob/v0.38.5/test/e2e/pkg/manifest.go for inspiration.

use anyhow::{bail, Context};
use fvm_shared::econ::TokenAmount;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{collections::BTreeMap, path::Path};
use url::Url;

use fendermint_vm_encoding::IsHumanReadable;
use fendermint_vm_genesis::Collateral;

use crate::{validation::validate_manifest, AccountId, NodeId, RelayerId, SubnetId, TestnetName};

pub type SubnetMap = BTreeMap<SubnetId, Subnet>;
pub type BalanceMap = BTreeMap<AccountId, Balance>;
pub type CollateralMap = BTreeMap<AccountId, Collateral>;
pub type NodeMap = BTreeMap<NodeId, Node>;
pub type RelayerMap = BTreeMap<RelayerId, Relayer>;
pub type EnvMap = BTreeMap<String, String>;

/// The manifest is a static description of a testnet.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// All the accounts we want to act with across the entire subnet hierarchy.
    ///
    /// Each account will have its pair of private and public keys.
    ///
    /// In the rootnet, if we are dealing with Calibration, they will get their
    /// initial balance from the Faucet, which should give 100 tFIL ("testnet" FIL),
    /// which is why there is no definition for the account balances for the root.
    ///
    /// This would be different if we deployed a root in the test, e.g. using
    /// Fendermint itself, in which case we could set whatever balance we wanted.
    pub accounts: BTreeMap<AccountId, Account>,

    /// Whether we use an existing L1 or create or own.
    pub rootnet: Rootnet,

    /// Subnets created on the rootnet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub subnets: SubnetMap,
}

impl Manifest {
    /// Read a manifest from file. It chooses the format based on the extension.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let Some(ext) = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
        else {
            bail!("manifest file has no extension, cannot determine format");
        };

        let manifest = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest from {}", path.to_string_lossy()))?;

        match ext.as_str() {
            "yaml" => serde_yaml::from_str(&manifest).context("failed to parse manifest YAML"),
            "json" => serde_json::from_str(&manifest).context("failed to parse manifest JSON"),
            "toml" => toml::from_str(&manifest).context("failed to parse manifest TOML"),
            other => bail!("unknown manifest format: {other}"),
        }
    }

    /// Perform sanity checks.
    pub async fn validate(&self, name: &TestnetName) -> anyhow::Result<()> {
        validate_manifest(name, self).await
    }
}

/// Any potential attributes of an account.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {}

/// Account balance.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Balance(#[serde_as(as = "IsHumanReadable")] pub TokenAmount);

/// Ways we can hook up with IPC contracts on the rootnet.
///
/// The rootnet is generally expected to be Calibration net,
/// where IPC contracts are deployed from Hardhat, and multiple
/// instances of the gateway exist, each with a different version
/// and an address we learn after deployment.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum IpcDeployment {
    /// Deploy a new IPC contract stack using one of the accounts.
    /// This can take a long time, but ensures we are testing with
    /// contracts that have the same version as the client.
    New { deployer: AccountId },
    /// Use one of the existing deployments, given by the delegated address of
    /// the Gateway and Registry contracts.
    Existing {
        gateway: ethers::core::types::Address,
        registry: ethers::core::types::Address,
    },
}

/// The rootnet, ie. the L1 chain, can already exist and be outside our control
/// if we are deploying to Calibration, or it might be a chain we provision
/// with CometBFT and Fendermint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Rootnet {
    /// Existing L1 running outside our control.
    ///
    /// This implies using some sort of Faucet to get balances for the accounts.
    External {
        /// We need to know the ID of the chain to be able to create a `SubnetID` for it.
        chain_id: u64,
        /// Indicate whether we have to (re)deploy the IPC contract or we can use an existing one.
        deployment: IpcDeployment,
        /// Addresses of JSON-RPC endpoints on the external L1.
        urls: Vec<Url>,
    },

    /// Provision a new chain to run the L1.
    ///
    /// It is assumed that a newly provisioned chain will have built-in support for IPC,
    /// e.g. the way Fendermint deploys IPC actors at well-known addresses.
    New {
        /// Collateral of the initial validator set.
        validators: CollateralMap,
        /// Balances of the accounts in the rootnet.
        ///
        /// These balances will go in the genesis file.
        balances: BalanceMap,
        /// Nodes that participate in running the root chain.
        nodes: NodeMap,
        /// Custom env vars to pass on to the nodes.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: EnvMap,
    },
}

/// An IPC subnet.
///
/// The balance of the account on the parent subnet, as declared in this manifest,
/// _does not_ have to account for the collateral/balance we have to take from it to join/fund the subnet.
/// When we create the testnet configuration from the manifest we will account for this with a rollup,
/// so we don't have to do that much mental arithmetic and run into frustrating errors during setup.
/// If we want to test trying to join with more than what we have, we can do so in the integration test.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subnet {
    /// The account we use to create the subnet.
    pub creator: AccountId,
    /// Collateral of the initial validator set.
    ///
    /// These validators will join the subnet with these collaterals after the subnet is created.
    pub validators: CollateralMap,
    /// Balances of the accounts at the creation of the subnet.
    ///
    /// These accounts will pre-fund the subnet after it's created.
    pub balances: BalanceMap,
    /// Nodes that participate in running the chain of this subnet.
    pub nodes: NodeMap,
    /// Relayers that submit bottom-up checkpoints to the parent subnet.
    pub relayers: RelayerMap,
    /// Bottom-up checkpoint configuration.
    pub bottom_up_checkpoint: CheckpointConfig,
    /// Custom env vars to pass on to the nodes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: EnvMap,
    /// Child subnets under this parent.
    ///
    /// The subnet ID exists so we can find the outcome of existing deployments in the log.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub subnets: SubnetMap,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Node {
    /// Indicate whether this is a validator node or a full node.
    pub mode: NodeMode,
    /// Indicate whether to run the Ethereum API.
    pub ethapi: bool,
    /// The nodes from which CometBFT should bootstrap itself.
    ///
    /// We can leave it empty for standalone nodes and in cases
    /// where we don't want mutual seeding, however it's best to
    /// still show the field in the manifest explicitly, to make
    /// sure it's not forgotten, which would prevent the nodes
    /// discovering each other.
    pub seed_nodes: Vec<NodeId>,
    /// The parent node that the top-down syncer follows;
    /// or leave it empty if node is on the rootnet.
    ///
    /// We can skip this field if it's empty because validation
    /// will tell us that all subnet nodes need a parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_node: Option<ParentNode>,
}

/// The mode in which CometBFT is running.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum NodeMode {
    /// A node able to create and sign blocks.
    Validator { validator: AccountId },
    /// A node which executes blocks and checks their content, but doesn't have a validator key.
    Full,
    // TODO: We can expand this to include seed nodes.
}

/// A node on the parent subnet.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ParentNode {
    /// An external node such as one on Calibnet, given by its JSON-RPC URL.
    External(Url),
    /// A node defined in the manifest.
    Internal(NodeId),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relayer {
    /// The account which will pay for the submission on the parent subnet.
    pub submitter: AccountId,
    /// The node which the relayer is following on the subnet.
    pub follow_node: NodeId,
    /// The node where the relayer submits the checkpoints;
    /// or leave it empty if the parent is CalibrationNet.
    pub submit_node: ParentNode,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointConfig {
    /// Number of blocks between checkpoints.
    pub period: u64,
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::Manifest;

    #[quickcheck]
    fn manifest_json(value0: Manifest) {
        let repr = serde_json::to_string(&value0).expect("failed to encode");
        let value1: Manifest = serde_json::from_str(&repr)
            .map_err(|e| format!("{e}; {repr}"))
            .expect("failed to decode JSON");

        assert_eq!(value1, value0)
    }

    #[quickcheck]
    fn manifest_yaml(value0: Manifest) {
        let repr = serde_yaml::to_string(&value0).expect("failed to encode");
        let value1: Manifest = serde_yaml::from_str(&repr)
            .map_err(|e| format!("{e}; {repr}"))
            .expect("failed to decode");

        assert_eq!(value1, value0)
    }

    #[quickcheck]
    fn manifest_toml(value0: Manifest) {
        let repr = toml::to_string(&value0).expect("failed to encode");
        let value1: Manifest = toml::from_str(&repr)
            .map_err(|e| format!("{e}; {repr}"))
            .expect("failed to decode");

        assert_eq!(value1, value0)
    }
}
