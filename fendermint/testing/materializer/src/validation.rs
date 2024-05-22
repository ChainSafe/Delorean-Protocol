// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::{anyhow, bail, Ok};
use async_trait::async_trait;
use either::Either;
use ethers::types::H160;
use fendermint_vm_genesis::Collateral;
use fvm_shared::{chainid::ChainID, econ::TokenAmount};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Debug,
    ops::{Add, Sub},
};
use url::Url;

use crate::{
    logging::LoggingMaterializer,
    manifest::{Balance, Manifest},
    materializer::{Materializer, NodeConfig, RelayerConfig, SubmitConfig, SubnetConfig},
    materials::Materials,
    testnet::Testnet,
    AccountName, NodeName, RelayerName, ResourceHash, ResourceName, SubnetName, TestnetName,
};

const DEFAULT_FAUCET_FIL: u64 = 100;

/// Do simple sanity checks on the manifest, e.g.:
/// * we are not over allocating the balances
/// * relayers have balances on the parent to submit transactions
/// * subnet creators have balances on the parent to submit transactions
pub async fn validate_manifest(name: &TestnetName, manifest: &Manifest) -> anyhow::Result<()> {
    let m = ValidatingMaterializer::default();
    // Wrap with logging so that we can debug the tests easier.
    let mut m = LoggingMaterializer::new(m, "validation".to_string());
    let _ = Testnet::setup(&mut m, name, manifest).await?;
    // We could check here that all subnets have enough validators for a quorum.
    Ok(())
}

pub struct ValidationMaterials;

impl Materials for ValidationMaterials {
    type Network = TestnetName;
    type Deployment = SubnetName;
    type Account = AccountName;
    type Genesis = SubnetName;
    type Subnet = SubnetName;
    type Node = NodeName;
    type Relayer = RelayerName;
}

type VNetwork = <ValidationMaterials as Materials>::Network;
type VDeployment = <ValidationMaterials as Materials>::Deployment;
type VAccount = <ValidationMaterials as Materials>::Account;
type VGenesis = <ValidationMaterials as Materials>::Genesis;
type VSubnet = <ValidationMaterials as Materials>::Subnet;
type VNode = <ValidationMaterials as Materials>::Node;
type VRelayer = <ValidationMaterials as Materials>::Relayer;

#[derive(Clone, Debug, Default)]
pub struct ValidatingMaterializer {
    network: Option<TestnetName>,
    balances: BTreeMap<SubnetName, BTreeMap<AccountName, TokenAmount>>,
    references: BTreeMap<SubnetName, HashSet<ResourceHash>>,
}

impl ValidatingMaterializer {
    fn network(&self) -> anyhow::Result<TestnetName> {
        self.network
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow!("network isn't set"))
    }

    /// Check that a name is within the subnet. This should trivially be true by construction, but still.
    fn ensure_contains<T: AsRef<ResourceName> + Debug>(&self, name: &T) -> anyhow::Result<()> {
        let tn = self.network()?;
        if !tn.contains(name) {
            bail!("{tn:?} does not contain {name:?}");
        }
        Ok(())
    }

    /// Ensure we aren't reusing references.
    fn ensure_unique(
        &mut self,
        subnet: &SubnetName,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()> {
        if let Some(r) = reference {
            let rs = self.references.entry(subnet.clone()).or_default();
            if !rs.insert(r) {
                bail!("a reference is reused in {subnet:?}");
            }
        }
        Ok(())
    }

    /// Check that an account has a positive balance on a subnet
    fn ensure_balance(&self, subnet: &SubnetName, account: &AccountName) -> anyhow::Result<()> {
        match self.balances.get(subnet) {
            None => bail!("{subnet:?} has not been created"),
            Some(bs) => match bs.get(account) {
                None => bail!("{account:?} has no balance on {subnet:?}"),
                Some(b) if b.is_zero() => bail!("{account:?} has zero balance on {subnet:?}"),
                Some(_) => Ok(()),
            },
        }
    }

    /// Check that the subnet has been created already.
    fn ensure_subnet_exists(&self, subnet: &SubnetName) -> anyhow::Result<()> {
        if !self.balances.contains_key(subnet) {
            bail!("{subnet:?} has not been created");
        }
        Ok(())
    }

    /// Move funds of an account from the parent to the child subnet.
    ///
    /// Fails if either:
    /// * the parent doesn't exist
    /// * the child doesn't exist
    /// * the account doesn't have the funds
    fn fund_from_parent(
        &mut self,
        subnet: &SubnetName,
        account: &AccountName,
        amount: TokenAmount,
        credit_child: bool,
    ) -> anyhow::Result<()> {
        let parent = subnet
            .parent()
            .ok_or_else(|| anyhow!("{subnet} must have a parent to fund from"))?;

        self.ensure_subnet_exists(&parent)?;
        self.ensure_subnet_exists(subnet)?;

        if amount.is_zero() {
            return Ok(());
        }

        self.ensure_balance(&parent, account)?;

        let pbs = self.balances.get_mut(&parent).unwrap();
        let pb = pbs.get_mut(account).unwrap();

        if *pb < amount {
            bail!("{account:?} has less than {amount} on {parent:?}, cannot fund {subnet:?}");
        }
        *pb = pb.clone().sub(amount.clone());

        if credit_child {
            let cbs = self.balances.get_mut(subnet).unwrap();
            let cb = cbs.entry(account.clone()).or_default();
            *cb = cb.clone().add(amount);
        }

        Ok(())
    }
}

#[async_trait]
impl Materializer<ValidationMaterials> for ValidatingMaterializer {
    async fn create_network(&mut self, testnet_name: &TestnetName) -> anyhow::Result<VNetwork> {
        self.network = Some(testnet_name.clone());
        Ok(testnet_name.clone())
    }

    fn create_account(&mut self, account_name: &AccountName) -> anyhow::Result<VAccount> {
        self.ensure_contains(account_name)?;
        Ok(account_name.clone())
    }

    async fn fund_from_faucet<'s, 'a>(
        &'s mut self,
        account: &'a VAccount,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        let tn = self.network()?;
        self.ensure_unique(&tn.root(), reference)?;
        let balances = self.balances.entry(tn.root()).or_default();
        let balance = balances.entry(account.clone()).or_default();

        *balance = balance
            .clone()
            .add(TokenAmount::from_whole(DEFAULT_FAUCET_FIL));

        Ok(())
    }

    async fn new_deployment<'s, 'a>(
        &'s mut self,
        subnet_name: &SubnetName,
        deployer: &'a VAccount,
        _urls: Vec<Url>,
    ) -> anyhow::Result<VDeployment>
    where
        's: 'a,
    {
        self.ensure_contains(subnet_name)?;
        self.ensure_balance(subnet_name, deployer)?;
        Ok(subnet_name.clone())
    }

    fn existing_deployment(
        &mut self,
        subnet_name: &SubnetName,
        gateway: H160,
        registry: H160,
    ) -> anyhow::Result<VDeployment> {
        self.ensure_contains(subnet_name)?;

        if gateway == registry {
            bail!("gateway and registry addresses are the same in {subnet_name:?}: {gateway} == {registry}");
        }

        Ok(subnet_name.clone())
    }

    fn default_deployment(&mut self, subnet_name: &SubnetName) -> anyhow::Result<VDeployment> {
        self.ensure_contains(subnet_name)?;
        Ok(subnet_name.clone())
    }

    fn create_root_genesis<'a>(
        &mut self,
        subnet_name: &SubnetName,
        validators: BTreeMap<&'a VAccount, Collateral>,
        balances: BTreeMap<&'a VAccount, Balance>,
    ) -> anyhow::Result<VGenesis> {
        self.ensure_contains(subnet_name)?;
        let tn = self.network()?;

        if validators.is_empty() {
            bail!("validators of {subnet_name:?} cannot be empty");
        }

        let root_balances = self.balances.entry(tn.root()).or_default();

        for (n, b) in balances {
            let balance = root_balances.entry(n.clone()).or_default();
            *balance = b.0;
        }

        Ok(subnet_name.clone())
    }

    fn create_root_subnet(
        &mut self,
        subnet_name: &SubnetName,
        _params: Either<ChainID, &VGenesis>,
    ) -> anyhow::Result<VSubnet> {
        Ok(subnet_name.clone())
    }

    async fn create_node<'s, 'a>(
        &'s mut self,
        node_name: &NodeName,
        _node_config: &NodeConfig<'a, ValidationMaterials>,
    ) -> anyhow::Result<VNode>
    where
        's: 'a,
    {
        self.ensure_contains(node_name)?;
        Ok(node_name.clone())
    }

    async fn start_node<'s, 'a>(
        &'s mut self,
        _node: &'a VNode,
        _seed_nodes: &'a [&'a VNode],
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn create_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, ValidationMaterials>,
        subnet_name: &SubnetName,
        subnet_config: &SubnetConfig<'a, ValidationMaterials>,
    ) -> anyhow::Result<VSubnet>
    where
        's: 'a,
    {
        self.ensure_contains(subnet_name)?;
        // Check that the submitter has balance on the parent subnet to create the child.
        let parent = parent_submit_config.subnet;
        self.ensure_balance(parent, subnet_config.creator)?;
        // Insert child subnet balances entry.
        self.balances
            .insert(subnet_name.clone(), Default::default());
        Ok(subnet_name.clone())
    }

    async fn fund_subnet<'s, 'a>(
        &'s mut self,
        _parent_submit_config: &SubmitConfig<'a, ValidationMaterials>,
        account: &'a VAccount,
        subnet: &'a VSubnet,
        amount: TokenAmount,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        // Debit parent balance; Credit child balance
        self.fund_from_parent(subnet, account, amount, true)?;
        self.ensure_unique(&subnet.parent().unwrap(), reference)?;
        Ok(())
    }

    async fn join_subnet<'s, 'a>(
        &'s mut self,
        _parent_submit_config: &SubmitConfig<'a, ValidationMaterials>,
        account: &'a VAccount,
        subnet: &'a VSubnet,
        collateral: Collateral,
        balance: Balance,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        // Debit parent balance, but do not make the funds available in the child
        self.fund_from_parent(subnet, account, collateral.0, false)?;
        // Debit parent balance; Credit child balance
        self.fund_from_parent(subnet, account, balance.0, true)?;
        self.ensure_unique(&subnet.parent().unwrap(), reference)?;
        Ok(())
    }

    async fn create_subnet_genesis<'s, 'a>(
        &'s mut self,
        _parent_submit_config: &SubmitConfig<'a, ValidationMaterials>,
        subnet: &'a VSubnet,
    ) -> anyhow::Result<VGenesis>
    where
        's: 'a,
    {
        // We're supposed to fetch the data from the parent, there's nothing to check.
        Ok(subnet.clone())
    }

    async fn create_relayer<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, ValidationMaterials>,
        relayer_name: &RelayerName,
        relayer_config: RelayerConfig<'a, ValidationMaterials>,
    ) -> anyhow::Result<VRelayer>
    where
        's: 'a,
    {
        self.ensure_contains(relayer_name)?;
        // Check that submitter has balance on the parent.
        let parent = parent_submit_config.subnet;
        self.ensure_balance(parent, relayer_config.submitter)?;
        Ok(relayer_name.clone())
    }
}

#[cfg(test)]
mod tests {

    use crate::{manifest::Manifest, validation::validate_manifest, TestnetId, TestnetName};

    // Unfortunately doesn't seem to work with quickcheck_async
    // /// Run the tests with `RUST_LOG=info` to see the logs, for example:
    // ///
    // /// ```text
    // /// RUST_LOG=info cargo test -p fendermint_testing_materializer prop_validation -- --nocapture
    // /// ```
    // fn init_log() {
    //     let _ = env_logger::builder().is_test(true).try_init();
    // }

    /// Check that the random manifests we generate would pass validation.
    #[quickcheck_async::tokio]
    async fn prop_validation(id: TestnetId, manifest: Manifest) -> anyhow::Result<()> {
        let name = TestnetName::new(id);
        validate_manifest(&name, &manifest).await
    }
}
