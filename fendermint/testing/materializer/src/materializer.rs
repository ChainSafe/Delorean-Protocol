// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use async_trait::async_trait;
use either::Either;
use ethers::types::H160;
use fvm_shared::{chainid::ChainID, econ::TokenAmount};
use std::collections::BTreeMap;
use url::Url;

use fendermint_vm_genesis::Collateral;

use crate::{
    manifest::{Balance, CheckpointConfig, EnvMap},
    materials::Materials,
    AccountName, NodeName, RelayerName, ResourceHash, SubnetName, TestnetName,
};

/// The materializer is a component to provision resources of a testnet, and
/// to carry out subsequent commands on them, e.g. to restart nodes.
///
/// By contrast, the role of the [Testnet] is to keep related items organised
/// and accessible for the integration tests, carrying out the operations with
/// the help of the materializer, which should keep the [Testnet] itself testable.
///
/// The materializer might not actually instantiate the resources. By returning
/// abstract types instead of concrete values, it is possible to just collect the
/// operations and use them to validate the behaviour of whatever is driving
/// the materializer. We can use this for dry-runs as well.
///
/// A live materializer should persist its logs, so that it can be resumed.
/// For example we can create and run a testnet externally, then parse the manifest
/// and the materializer logs inside a test to talk to one of the nodes, and the
/// materializer should be able to return to the test correct JSON-RPC endpoints.
///
/// Some of the operations of the materializer should be idempotent, e.g. the
/// creation of a wallet or a node should only happen once.
///
/// The types returned might have their own logic to execute when dropped, to free
/// resources. This might happen only if the resource is not an externally managed
/// one, e.g. a testnet set up before tests are run, which the materializer should
/// know.
#[async_trait]
pub trait Materializer<M: Materials> {
    /// Create the physical network group.
    ///
    /// The return value should be able to able to represent settings that allow nodes
    /// to connect to each other, as well as perhaps to be labelled as a group
    /// (although for that we can use the common name prefixes as well).
    async fn create_network(&mut self, testnet_name: &TestnetName) -> anyhow::Result<M::Network>;

    /// Create a Secp256k1 keypair for signing transactions or creating blocks.
    fn create_account(&mut self, account_name: &AccountName) -> anyhow::Result<M::Account>;

    /// Fund an account on the rootnet from the faucet.
    async fn fund_from_faucet<'s, 'a>(
        &'s mut self,
        account: &'a M::Account,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a;

    /// Deploy the IPC contracts onto the rootnet.
    ///
    /// This is assumed to be used with external subnets, with the API address
    /// being known to the materializer, but not being part of the manifest,
    /// as there can be multiple endpoints to choose from, some better than others.
    ///
    /// The return value should contain at the addresses of the contracts.
    async fn new_deployment<'s, 'a>(
        &'s mut self,
        subnet_name: &SubnetName,
        deployer: &'a M::Account,
        urls: Vec<Url>,
    ) -> anyhow::Result<M::Deployment>
    where
        's: 'a;

    /// Set the IPC contracts onto the rootnet.
    ///
    /// This is assumed to be used with external subnets, with the API address
    /// being known to the materializer, but not being part of the manifest,
    /// as there can be multiple endpoints to choose from, some better than others.
    fn existing_deployment(
        &mut self,
        subnet_name: &SubnetName,
        gateway: H160,
        registry: H160,
    ) -> anyhow::Result<M::Deployment>;

    /// Return the well-known IPC contract deployments.
    fn default_deployment(&mut self, subnet_name: &SubnetName) -> anyhow::Result<M::Deployment>;

    /// Construct the genesis for the rootnet.
    ///
    /// The genesis time and the chain name (which should determine the chain ID and
    /// thus the subnet ID as well) can be chosen by the materializer, or we could make
    /// it part of the manifest.
    fn create_root_genesis<'a>(
        &mut self,
        subnet_name: &SubnetName,
        validators: BTreeMap<&'a M::Account, Collateral>,
        balances: BTreeMap<&'a M::Account, Balance>,
    ) -> anyhow::Result<M::Genesis>;

    /// Create a subnet to represent the root.
    fn create_root_subnet(
        &mut self,
        subnet_name: &SubnetName,
        params: Either<ChainID, &M::Genesis>,
    ) -> anyhow::Result<M::Subnet>;

    /// Construct the configuration for a node.
    ///
    /// This should create keys, configurations, but hold on from starting so that we can
    /// first learn about the dynamic properties of other nodes in the cluster we depend on,
    /// such as their network identities which are a function of their keys.
    ///
    /// The method is async in case we have to provision some resources remotely.
    async fn create_node<'s, 'a>(
        &'s mut self,
        node_name: &NodeName,
        node_config: &NodeConfig<'a, M>,
    ) -> anyhow::Result<M::Node>
    where
        's: 'a;

    /// Start a node.
    ///
    /// At this point the identities of any dependency nodes should be known.
    async fn start_node<'s, 'a>(
        &'s mut self,
        node: &'a M::Node,
        seed_nodes: &'a [&'a M::Node],
    ) -> anyhow::Result<()>
    where
        's: 'a;

    /// Create a subnet on the parent subnet ledger.
    ///
    /// The parent nodes are the ones where subnet-creating transactions
    /// can be sent, or it can be empty if it's an external rootnet.
    ///
    /// The result should contain the address of the subnet.
    async fn create_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, M>,
        subnet_name: &SubnetName,
        subnet_config: &SubnetConfig<'a, M>,
    ) -> anyhow::Result<M::Subnet>
    where
        's: 'a;

    /// Fund an account on a target subnet by transferring tokens from the source subnet.
    ///
    /// Only works if the target subnet has been bootstrapped.
    ///
    /// The `reference` can be used to deduplicate repeated transfer attempts.
    async fn fund_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, M>,
        account: &'a M::Account,
        subnet: &'a M::Subnet,
        amount: TokenAmount,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a;

    /// Join a target subnet as a validator.
    ///
    /// The `reference` can be used to deduplicate repeated transfer attempts.
    async fn join_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, M>,
        account: &'a M::Account,
        subnet: &'a M::Subnet,
        collateral: Collateral,
        balance: Balance,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a;

    /// Construct the genesis for a subnet, which involves fetching details from the parent.
    ///
    /// The method is async to allow for network operations.
    async fn create_subnet_genesis<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, M>,
        subnet: &'a M::Subnet,
    ) -> anyhow::Result<M::Genesis>
    where
        's: 'a;

    /// Create and start a relayer.
    async fn create_relayer<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, M>,
        relayer_name: &RelayerName,
        relayer_config: RelayerConfig<'a, M>,
    ) -> anyhow::Result<M::Relayer>
    where
        's: 'a;
}

/// Options regarding node configuration, e.g. which services to start.
pub struct NodeConfig<'a, M: Materials> {
    /// The physical network to join.
    pub network: &'a M::Network,
    /// The genesis of this subnet; it should indicate whether this is a rootnet or a deeper level.
    pub genesis: &'a M::Genesis,
    /// The validator keys if this is a validator node; none if just a full node.
    pub validator: Option<&'a M::Account>,
    /// The node for the top-down syncer to follow; none if this is a root node.
    ///
    /// This can potentially also be used to configure the IPLD Resolver seeds, to connect across subnets.
    pub parent_node: Option<ParentConfig<'a, M>>,
    /// Run the Ethereum API facade or not.
    pub ethapi: bool,
    /// Arbitrary env vars, e.g. to regulate block production rates.
    pub env: &'a EnvMap,
    /// Number of nodes to be expected in the subnet, including this node, or 0 if unknown.
    pub peer_count: usize,
}

/// Options regarding relayer configuration
pub struct RelayerConfig<'a, M: Materials> {
    /// Where to send queries on the child subnet.
    pub follow_config: &'a SubmitConfig<'a, M>,
    /// The account to use to submit transactions on the parent subnet.
    pub submitter: &'a M::Account,
    /// Arbitrary env vars, e.g. to set the logging level.
    pub env: &'a EnvMap,
}

/// Options regarding subnet configuration, e.g. how many validators are required.
pub struct SubnetConfig<'a, M: Materials> {
    /// Which account to use on the parent to create the subnet.
    ///
    /// This account has to have the necessary balance on the parent.
    pub creator: &'a M::Account,
    /// Number of validators required for bootstrapping a subnet.
    pub min_validators: usize,
    pub bottom_up_checkpoint: &'a CheckpointConfig,
}

/// Options for how to submit IPC transactions to a subnet.
pub struct SubmitConfig<'a, M: Materials> {
    /// The nodes to which we can send transactions or queries, ie. any of the parent nodes.
    pub nodes: Vec<TargetConfig<'a, M>>,
    /// The identity of the subnet to which we submit the transaction, ie. the parent subnet.
    pub subnet: &'a M::Subnet,
    /// The location of the IPC contracts on the (generally parent) subnet.
    pub deployment: &'a M::Deployment,
}

/// Options for how to follow the parent consensus and sync IPC changes.
pub struct ParentConfig<'a, M: Materials> {
    /// The trusted parent node to follow.
    pub node: TargetConfig<'a, M>,
    /// The location of the IPC contracts on the parent subnet.
    pub deployment: &'a M::Deployment,
}

/// Where to submit a transaction or a query.
pub enum TargetConfig<'a, M: Materials> {
    External(Url),
    Internal(&'a M::Node),
}

impl<'a, M: Materials> SubmitConfig<'a, M> {
    /// Map over the internal and external target configurations to find a first non-empty result.
    pub fn find_node<F, G, T>(&self, f: F, g: G) -> Option<T>
    where
        F: Fn(&M::Node) -> Option<T>,
        G: Fn(&Url) -> Option<T>,
    {
        self.nodes
            .iter()
            .filter_map(|tc| match tc {
                TargetConfig::Internal(n) => f(n),
                TargetConfig::External(u) => g(u),
            })
            .next()
    }
}
