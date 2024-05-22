// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::{anyhow, bail, Context};
use async_recursion::async_recursion;
use either::Either;
use fvm_shared::chainid::ChainID;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    marker::PhantomData,
};
use url::Url;

use crate::{
    manifest::{
        BalanceMap, CollateralMap, EnvMap, IpcDeployment, Manifest, Node, NodeMode, ParentNode,
        Rootnet, Subnet,
    },
    materializer::{
        Materializer, NodeConfig, ParentConfig, RelayerConfig, SubmitConfig, SubnetConfig,
        TargetConfig,
    },
    materials::Materials,
    AccountId, NodeId, NodeName, RelayerName, ResourceHash, SubnetId, SubnetName, TestnetName,
};

/// The `Testnet` parses a [Manifest] and is able to derive the steps
/// necessary to instantiate it with the help of the [Materializer].
///
/// The `Testnet` data structure itself acts as an indexer over the
/// resources created by the [Materializer]. It owns them, and by
/// doing so controls their life cycle. By dropping the `Testnet`
/// or various components from it we are able to free resources.
///
/// Arguably the same could be achieved by keeping the created
/// resources inside the [Materializer] and discarding that as
/// a whole, keeping the `Testnet` completely stateless, but
/// perhaps this way writing a [Materializer] is just a tiny
/// bit simpler.
pub struct Testnet<M: Materials, R> {
    name: TestnetName,
    network: M::Network,
    externals: Vec<Url>,
    accounts: BTreeMap<AccountId, M::Account>,
    deployments: BTreeMap<SubnetName, M::Deployment>,
    genesis: BTreeMap<SubnetName, M::Genesis>,
    subnets: BTreeMap<SubnetName, M::Subnet>,
    nodes: BTreeMap<NodeName, M::Node>,
    relayers: BTreeMap<RelayerName, M::Relayer>,
    _phantom_materializer: PhantomData<R>,
}

impl<M: Materials, R> Drop for Testnet<M, R> {
    fn drop(&mut self) {
        // Make sure anything that can use a common network is dropped first.
        drop(std::mem::take(&mut self.relayers));
        drop(std::mem::take(&mut self.nodes));
    }
}

impl<M, R> Testnet<M, R>
where
    M: Materials,
    R: Materializer<M> + Sync + Send,
{
    pub async fn new(m: &mut R, name: &TestnetName) -> anyhow::Result<Self> {
        let network = m
            .create_network(name)
            .await
            .context("failed to create the network")?;

        Ok(Self {
            name: name.clone(),
            network,
            externals: Default::default(),
            accounts: Default::default(),
            deployments: Default::default(),
            genesis: Default::default(),
            subnets: Default::default(),
            nodes: Default::default(),
            relayers: Default::default(),
            _phantom_materializer: PhantomData,
        })
    }

    pub fn name(&self) -> &TestnetName {
        &self.name
    }

    pub fn root(&self) -> SubnetName {
        self.name.root()
    }

    /// Set up a testnet from scratch.
    ///
    /// To validate a manifest, we can first create a testnet with a [Materializer]
    /// that only creates symbolic resources.
    pub async fn setup(m: &mut R, name: &TestnetName, manifest: &Manifest) -> anyhow::Result<Self> {
        let mut t = Self::new(m, name).await?;
        let root_name = t.root();

        // Create keys for accounts.
        for account_id in manifest.accounts.keys() {
            t.create_account(m, account_id)?;
        }

        // Create the rootnet.
        t.create_and_start_rootnet(m, &root_name, &manifest.rootnet)
            .await
            .context("failed to create and start rootnet")?;

        // Recursively create and start all subnet nodes.
        for (subnet_id, subnet) in &manifest.subnets {
            t.create_and_start_subnet(m, &root_name, subnet_id, subnet)
                .await
                .with_context(|| format!("failed to create and start subnet {subnet_id}"))?;
        }

        Ok(t)
    }

    /// Return a reference to the physical network.
    fn network(&self) -> &M::Network {
        &self.network
    }

    /// Create a cryptographic keypair for an account ID.
    pub fn create_account(&mut self, m: &mut R, id: &AccountId) -> anyhow::Result<()> {
        let n = self.name.account(id);
        let a = m.create_account(&n).context("failed to create account")?;
        self.accounts.insert(id.clone(), a);
        Ok(())
    }

    /// Get an account by ID.
    pub fn account(&self, id: impl Into<AccountId>) -> anyhow::Result<&M::Account> {
        let id: AccountId = id.into();
        self.accounts
            .get(&id)
            .ok_or_else(|| anyhow!("account {id} does not exist"))
    }

    /// Get a node by name.
    pub fn node(&self, name: &NodeName) -> anyhow::Result<&M::Node> {
        self.nodes
            .get(name)
            .ok_or_else(|| anyhow!("node {name:?} does not exist"))
    }

    /// Get a subnet by name.
    pub fn subnet(&self, name: &SubnetName) -> anyhow::Result<&M::Subnet> {
        self.subnets
            .get(name)
            .ok_or_else(|| anyhow!("subnet {name:?} does not exist"))
    }

    /// Get a genesis by subnet.
    pub fn genesis(&self, name: &SubnetName) -> anyhow::Result<&M::Genesis> {
        self.genesis
            .get(name)
            .ok_or_else(|| anyhow!("genesis for {name:?} does not exist"))
    }

    /// Get a deployment by subnet.
    pub fn deployment(&self, name: &SubnetName) -> anyhow::Result<&M::Deployment> {
        self.deployments
            .get(name)
            .ok_or_else(|| anyhow!("deployment for {name:?} does not exist"))
    }

    /// List all the nodes in a subnet.
    pub fn nodes_by_subnet(&self, subnet_name: &SubnetName) -> Vec<&M::Node> {
        self.nodes
            .iter()
            .filter(|(node_name, _)| subnet_name.contains(node_name))
            .map(|(_, n)| n)
            .collect()
    }

    /// Iterate all the nodes in the testnet.
    pub fn nodes(&self) -> impl Iterator<Item = (&NodeName, &M::Node)> {
        self.nodes.iter()
    }

    /// Where can we send transactions and queries on a subnet.
    pub fn submit_config(&self, subnet_name: &SubnetName) -> anyhow::Result<SubmitConfig<M>> {
        let deployment = self.deployment(subnet_name)?;
        let subnet = self.subnet(subnet_name)?;

        let mut nodes = self
            .nodes_by_subnet(subnet_name)
            .into_iter()
            .map(TargetConfig::Internal)
            .collect::<Vec<_>>();

        if subnet_name.is_root() {
            nodes.extend(self.externals.iter().cloned().map(TargetConfig::External));
        }

        Ok(SubmitConfig {
            subnet,
            deployment,
            nodes,
        })
    }

    /// Resolve account IDs in a map to account references.
    fn account_map<T>(
        &self,
        m: BTreeMap<AccountId, T>,
    ) -> anyhow::Result<BTreeMap<&M::Account, T>> {
        m.into_iter()
            .map(|(id, x)| self.account(&id).map(|a| (a, x)))
            .collect()
    }

    /// Create a genesis for the rootnet nodes.
    ///
    /// On the rootnet the validator power comes out of thin air,
    /// ie. the balances don't have to cover it. On subnets this
    /// will be different, the collateral has to be funded.
    fn create_root_genesis(
        &mut self,
        m: &mut R,
        subnet_name: &SubnetName,
        validators: CollateralMap,
        balances: BalanceMap,
    ) -> anyhow::Result<()> {
        let validators = self
            .account_map(validators)
            .context("invalid root collaterals")?;

        let balances = self
            .account_map(balances)
            .context("invalid root balances")?;

        // Remember the genesis so we can potentially create more nodes later.
        let genesis = m.create_root_genesis(subnet_name, validators, balances)?;

        self.genesis.insert(subnet_name.clone(), genesis);

        Ok(())
    }

    /// Configure and start the nodes of a subnet.
    ///
    /// Fails if the genesis of this subnet hasn't been created yet.
    async fn create_and_start_nodes(
        &mut self,
        m: &mut R,
        subnet_name: &SubnetName,
        nodes: &BTreeMap<NodeId, Node>,
        env: &EnvMap,
    ) -> anyhow::Result<()> {
        let node_ids = sort_by_seeds(nodes).context("invalid root subnet topology")?;

        for (node_id, node) in node_ids.iter() {
            self.create_node(m, subnet_name, node_id, node, env, node_ids.len())
                .await
                .with_context(|| format!("failed to create node {node_id} in {subnet_name}"))?;
        }

        for (node_id, node) in node_ids.iter() {
            self.start_node(m, subnet_name, node_id, node)
                .await
                .with_context(|| format!("failed to start node {node_id} in {subnet_name}"))?;
        }

        Ok(())
    }

    /// Create the configuration of a node.
    ///
    /// Fails if the genesis hasn't been created yet.
    async fn create_node(
        &mut self,
        m: &mut R,
        subnet_name: &SubnetName,
        node_id: &NodeId,
        node: &Node,
        env: &EnvMap,
        peer_count: usize,
    ) -> anyhow::Result<()> {
        let genesis = self.genesis(subnet_name)?;
        let network = self.network();
        let node_name = subnet_name.node(node_id);

        let parent_node = match (subnet_name.parent(), &node.parent_node) {
            (Some(ps), Some(ParentNode::Internal(id))) => {
                let tc = TargetConfig::<M>::Internal(
                    self.node(&ps.node(id))
                        .with_context(|| format!("invalid parent node in {node_name:?}"))?,
                );
                let deployment = self.deployment(&ps)?;
                Some(ParentConfig {
                    node: tc,
                    deployment,
                })
            }
            (Some(ps), Some(ParentNode::External(url))) if ps.is_root() => {
                let tc = TargetConfig::External(url.clone());
                let deployment = self.deployment(&ps)?;
                Some(ParentConfig {
                    node: tc,
                    deployment,
                })
            }
            (Some(_), Some(ParentNode::External(_))) => {
                bail!("node {node_name:?} specifies external URL for parent, but it's on a non-root subnet")
            }
            (None, Some(_)) => {
                bail!("node {node_name:?} specifies parent node, but there is no parent subnet")
            }
            (Some(_), None) => {
                bail!("node {node_name:?} is on a subnet, but doesn't specify a parent node")
            }
            _ => None,
        };

        let node_config = NodeConfig {
            network,
            genesis,
            validator: match &node.mode {
                NodeMode::Full => None,
                NodeMode::Validator { validator } => {
                    let validator = self
                        .account(validator)
                        .with_context(|| format!("invalid validator in {node_name:?}"))?;
                    Some(validator)
                }
            },
            parent_node,
            ethapi: node.ethapi,
            env,
            peer_count,
        };

        let node = m
            .create_node(&node_name, &node_config)
            .await
            .context("failed to create node")?;

        self.nodes.insert(node_name, node);

        Ok(())
    }

    /// Start a node.
    ///
    /// Fails if the node hasn't been created yet.
    async fn start_node(
        &mut self,
        m: &mut R,
        subnet_name: &SubnetName,
        node_id: &NodeId,
        node: &Node,
    ) -> anyhow::Result<()> {
        let node_name = subnet_name.node(node_id);

        let seeds = node
            .seed_nodes
            .iter()
            .map(|s| self.node(&subnet_name.node(s)))
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to collect seeds for {node_name:?}"))?;

        let node = self.node(&node_name)?;

        m.start_node(node, &seeds)
            .await
            .with_context(|| format!("failed to start {node_name:?}"))?;

        Ok(())
    }

    async fn create_and_start_rootnet(
        &mut self,
        m: &mut R,
        root_name: &SubnetName,
        rootnet: &Rootnet,
    ) -> anyhow::Result<()> {
        match rootnet {
            Rootnet::External {
                chain_id,
                deployment,
                urls,
            } => {
                // Establish balances.
                for (id, a) in self.accounts.iter() {
                    let reference = ResourceHash::digest(format!("funding {id} from faucet"));
                    m.fund_from_faucet(a, Some(reference))
                        .await
                        .context("faucet failed")?;
                }

                // Establish root contract locations.
                let deployment = match deployment {
                    IpcDeployment::New { deployer } => {
                        let deployer = self.account(deployer).context("invalid deployer")?;
                        m.new_deployment(root_name, deployer, urls.clone())
                            .await
                            .context("failed to deploy IPC contracts")?
                    }
                    IpcDeployment::Existing { gateway, registry } => {
                        m.existing_deployment(root_name, *gateway, *registry)?
                    }
                };

                let subnet = m
                    .create_root_subnet(root_name, Either::Left(ChainID::from(*chain_id)))
                    .context("failed to create root subnet")?;

                self.subnets.insert(root_name.clone(), subnet);
                self.deployments.insert(root_name.clone(), deployment);
                self.externals.clone_from(urls);
            }
            Rootnet::New {
                validators,
                balances,
                nodes,
                env,
            } => {
                self.create_root_genesis(m, root_name, validators.clone(), balances.clone())
                    .context("failed to create root genesis")?;

                let genesis = self.genesis(root_name)?;
                let subnet = m
                    .create_root_subnet(root_name, Either::Right(genesis))
                    .context("failed to create root subnet")?;
                let deployment = m.default_deployment(root_name)?;

                self.subnets.insert(root_name.clone(), subnet);
                self.deployments.insert(root_name.clone(), deployment);

                self.create_and_start_nodes(m, root_name, nodes, env)
                    .await
                    .context("failed to start root nodes")?;
            }
        }
        Ok(())
    }

    #[async_recursion]
    async fn create_and_start_subnet(
        &mut self,
        m: &mut R,
        parent_subnet_name: &SubnetName,
        subnet_id: &SubnetId,
        subnet: &Subnet,
    ) -> anyhow::Result<()> {
        let subnet_name = parent_subnet_name.subnet(subnet_id);

        // Create the subnet
        {
            // Assume that all subnets are deployed with the default contracts.
            self.deployments
                .insert(subnet_name.clone(), m.default_deployment(&subnet_name)?);

            // Where can we reach the gateway and the registry.
            let parent_submit_config = self.submit_config(parent_subnet_name)?;

            // Create the subnet on the parent.
            let created_subnet = m
                .create_subnet(
                    &parent_submit_config,
                    &subnet_name,
                    &SubnetConfig {
                        creator: self.account(&subnet.creator).context("invalid creator")?,
                        // Make the number such that the last validator to join activates the subnet.
                        min_validators: subnet.validators.len(),
                        bottom_up_checkpoint: &subnet.bottom_up_checkpoint,
                    },
                )
                .await
                .with_context(|| format!("failed to create {subnet_name}"))?;

            self.subnets.insert(subnet_name.clone(), created_subnet);
        };

        // Fund the accounts, join the subnet, start the nodes
        {
            let parent_submit_config = self.submit_config(parent_subnet_name)?;
            let created_subnet = self.subnet(&subnet_name)?;

            // Fund validator and balances collateral all the way from the root down to the parent.
            for (fund_source, fund_target) in subnet_name.ancestor_hops(false) {
                // Where can we send the subnet request.
                let fund_submit_config = self.submit_config(&fund_source)?;

                // Which subnet are we funding.
                let fund_subnet = self.subnet(&fund_target)?;

                let cs = subnet
                    .validators
                    .iter()
                    .map(|(id, c)| ("validator", id, c.0.clone()));

                let bs = subnet
                    .balances
                    .iter()
                    .map(|(id, b)| ("balance", id, b.0.clone()));

                for (label, id, amount) in cs.chain(bs) {
                    let account = self
                        .account(id)
                        .with_context(|| format!("invalid {label} in {subnet_name}"))?;

                    // Assign a reference so we can remember that we did it, within each subnet,
                    // which can turn this into an idempotent operation.
                    let reference = ResourceHash::digest(format!(
                        "funds from the top for {label} {id} for {subnet_name}"
                    ));

                    m.fund_subnet(
                        &fund_submit_config,
                        account,
                        fund_subnet,
                        amount,
                        Some(reference),
                    )
                    .await
                    .with_context(|| format!("failed to fund {id} in {fund_target:?}"))?;
                }
            }

            // Join with the validators on the subnet.
            for (id, c) in &subnet.validators {
                let account = self
                    .account(id)
                    .with_context(|| format!("invalid validator {id} in {subnet_name}"))?;

                let b = subnet.balances.get(id).cloned().unwrap_or_default();

                let reference =
                    ResourceHash::digest(format!("initial join by {id} for {subnet_name}"));

                m.join_subnet(
                    &parent_submit_config,
                    account,
                    created_subnet,
                    c.clone(),
                    b,
                    Some(reference),
                )
                .await
                .with_context(|| format!("failed to join with validator {id} in {subnet_name}"))?;
            }

            // Create genesis by fetching from the parent.
            let genesis = m
                .create_subnet_genesis(&parent_submit_config, created_subnet)
                .await
                .with_context(|| format!("failed to create subnet genesis in {subnet_name}"))?;

            self.genesis.insert(subnet_name.clone(), genesis);

            // Create and start nodes.
            self.create_and_start_nodes(m, &subnet_name, &subnet.nodes, &subnet.env)
                .await
                .with_context(|| format!("failed to start subnet nodes in {subnet_name}"))?;
        }

        // Interact with the running subnet.
        {
            let created_subnet = self.subnet(&subnet_name)?;
            let created_deployment = self.deployment(&subnet_name)?;

            // Where can we reach the gateway and the registry.
            let parent_submit_config = self.submit_config(parent_subnet_name)?;

            // Fund all non-validator balances (which have been passed to join_validator as a pre-fund request).
            // These could be done as pre-funds if the command is available on its own.
            for (id, b) in &subnet.balances {
                let account = self
                    .account(id)
                    .with_context(|| format!("invalid balance in {subnet_name}"))?;

                if subnet.validators.contains_key(id) {
                    continue;
                }

                let reference = ResourceHash::digest(format!("fund {id} in {subnet_name}"));

                m.fund_subnet(
                    &parent_submit_config,
                    account,
                    created_subnet,
                    b.0.clone(),
                    Some(reference),
                )
                .await
                .with_context(|| format!("failed to fund {id} in {subnet_name}"))?;
            }

            // Create relayers for bottom-up checkpointing.
            let mut relayers = Vec::<(RelayerName, M::Relayer)>::new();
            for (id, relayer) in &subnet.relayers {
                let submitter = self
                    .account(&relayer.submitter)
                    .context("invalid relayer")?;

                let follow_node = self
                    .node(&subnet_name.node(&relayer.follow_node))
                    .context("invalid follow node")?;

                let submit_node = match (subnet_name.parent(), &relayer.submit_node) {
                    (Some(p), ParentNode::Internal(s)) => TargetConfig::Internal(self.node(&p.node(s)).context("invalid submit node")?),
                    (Some(p), ParentNode::External(url)) if p.is_root() => TargetConfig::External(url.clone()),
                    (Some(_), ParentNode::External(_))  => bail!(
                        "invalid relayer {id} in {subnet_name}: parent is not root, but submit node is external"
                    ),
                    (None, _) => bail!(
                        "invalid relayer {id} in {subnet_name}: there is no parent subnet to relay to"
                    ),
                };

                let relayer_name = subnet_name.relayer(id);
                let relayer = m
                    .create_relayer(
                        &SubmitConfig {
                            nodes: vec![submit_node],
                            ..parent_submit_config
                        },
                        &relayer_name,
                        RelayerConfig {
                            follow_config: &SubmitConfig {
                                nodes: vec![TargetConfig::Internal(follow_node)],
                                subnet: created_subnet,
                                deployment: created_deployment,
                            },
                            submitter,
                            env: &subnet.env,
                        },
                    )
                    .await
                    .with_context(|| format!("failed to create relayer {id}"))?;

                relayers.push((relayer_name, relayer));
            }
            self.relayers.extend(relayers.into_iter());
        }

        // Recursively create and start all subnet nodes.
        for (subnet_id, subnet) in &subnet.subnets {
            self.create_and_start_subnet(m, &subnet_name, subnet_id, subnet)
                .await
                .with_context(|| format!("failed to start subnet {subnet_id} in {subnet_name}"))?;
        }

        Ok(())
    }
}

/// Sort some values in a topological order.
///
/// Cycles can be allowed, in which case it will do its best to order the items
/// with the least amount of dependencies first. This is so we can support nodes
/// mutually be seeded by each other.
fn topo_sort<K, V, F, I>(
    items: &BTreeMap<K, V>,
    allow_cycles: bool,
    f: F,
) -> anyhow::Result<Vec<(&K, &V)>>
where
    F: Fn(&V) -> I,
    K: Ord + Display + Clone,
    I: IntoIterator<Item = K>,
{
    let mut deps = items
        .iter()
        .map(|(k, v)| (k, BTreeSet::from_iter(f(v))))
        .collect::<BTreeMap<_, _>>();

    for (k, ds) in deps.iter() {
        for d in ds {
            if !deps.contains_key(d) {
                bail!("non-existing dependency: {d} <- {k}")
            }
        }
    }

    let mut sorted = Vec::new();

    while !deps.is_empty() {
        let leaf: K = match deps.iter().find(|(_, ds)| ds.is_empty()) {
            Some((leaf, _)) => (*leaf).clone(),
            None if allow_cycles => {
                let mut dcs = deps.iter().map(|(k, ds)| (k, ds.len())).collect::<Vec<_>>();
                dcs.sort_by_key(|(_, c)| *c);
                let leaf = dcs.first().unwrap().0;
                (*leaf).clone()
            }
            None => bail!("circular reference in dependencies"),
        };

        deps.remove(&leaf);

        for (_, ds) in deps.iter_mut() {
            ds.remove(&leaf);
        }

        if let Some(kv) = items.get_key_value(&leaf) {
            sorted.push(kv);
        }
    }

    Ok(sorted)
}

/// Sort nodes in a subnet in topological order, so we strive to first
/// start the ones others use as a seed node. However, do allow cycles
/// so that we can have nodes mutually bootstrap from each other.
fn sort_by_seeds(nodes: &BTreeMap<NodeId, Node>) -> anyhow::Result<Vec<(&NodeId, &Node)>> {
    topo_sort(nodes, true, |n| {
        BTreeSet::from_iter(n.seed_nodes.iter().cloned())
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::topo_sort;

    #[test]
    fn test_topo_sort() {
        let mut tree = BTreeMap::default();

        tree.insert(1, vec![]);
        tree.insert(2, vec![5]);
        tree.insert(3, vec![1, 5]);
        tree.insert(4, vec![2, 3]);
        tree.insert(5, vec![1]);

        let sorted = topo_sort(&tree, false, |ds| ds.clone())
            .unwrap()
            .into_iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();

        assert_eq!(sorted, vec![1, 5, 2, 3, 4]);

        tree.insert(1, vec![5]);

        topo_sort(&tree, false, |ds| ds.clone()).expect_err("shouldn't allow cycles");

        let sorted = topo_sort(&tree, true, |ds| ds.clone()).expect("should allow cycles");
        assert_eq!(sorted.len(), tree.len());
    }
}
