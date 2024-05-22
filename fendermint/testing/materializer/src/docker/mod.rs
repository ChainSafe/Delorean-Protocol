// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, bail, Context};
use async_trait::async_trait;
use bollard::{
    container::{ListContainersOptions, RemoveContainerOptions},
    network::ListNetworksOptions,
    secret::{ContainerSummary, Network},
    Docker,
};
use either::Either;
use ethers::{
    core::rand::{rngs::StdRng, SeedableRng},
    types::H160,
};
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_core::{chainid, Timestamp};
use fendermint_vm_genesis::{
    ipc::{GatewayParams, IpcParams},
    Account, Actor, ActorMeta, Collateral, Genesis, SignerAddr, Validator, ValidatorKey,
};
use fvm_shared::{bigint::Zero, chainid::ChainID, econ::TokenAmount, version::NetworkVersion};
use ipc_api::subnet_id::SubnetID;
use ipc_provider::config::subnet::{
    EVMSubnet, Subnet as IpcCliSubnet, SubnetConfig as IpcCliSubnetConfig,
};
use ipc_provider::config::Config as IpcCliConfig;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use url::Url;

use crate::{
    manifest::Balance,
    materializer::{
        Materializer, NodeConfig, RelayerConfig, SubmitConfig, SubnetConfig, TargetConfig,
    },
    materials::{
        export_file, export_json, export_script, import_json, DefaultAccount, DefaultDeployment,
        DefaultGenesis, DefaultSubnet, Materials,
    },
    CliName, NodeName, RelayerName, ResourceHash, ResourceName, SubnetName, TestnetName,
    TestnetResource,
};

mod container;
mod dropper;
mod network;
mod node;
mod relayer;
mod runner;

pub use dropper::DropPolicy;
pub use network::DockerNetwork;
pub use node::DockerNode;
pub use relayer::DockerRelayer;

use self::{dropper::DropHandle, network::NetworkName, runner::DockerRunner};

// TODO: Add these to the materializer.
const COMETBFT_IMAGE: &str = "cometbft/cometbft:v0.37.x";
const FENDERMINT_IMAGE: &str = "fendermint:latest";

const STATE_JSON_FILE_NAME: &str = "materializer-state.json";

const DOCKER_ENTRY_SCRIPT: &str = include_str!("../../scripts/docker-entry.sh");
const DOCKER_ENTRY_FILE_NAME: &str = "docker-entry.sh";

const PORT_RANGE_START: u32 = 30000;
const PORT_RANGE_SIZE: u32 = 100;

lazy_static! {
    static ref STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
}

type Volumes = Vec<(PathBuf, &'static str)>;

#[macro_export]
macro_rules! env_vars {
    ( $($key:literal => $value:expr),* $(,)? ) => {
        BTreeMap::from([ $( ($key.to_string(), $value.to_string()) ),* ])
    };
}

pub struct DockerMaterials;

impl Materials for DockerMaterials {
    type Deployment = DefaultDeployment;
    type Account = DefaultAccount;
    type Genesis = DefaultGenesis;
    type Subnet = DefaultSubnet;

    type Network = DockerNetwork;
    type Node = DockerNode;
    type Relayer = DockerRelayer;
}

/// A thing constructed by docker.
#[derive(Debug, Clone)]
pub struct DockerConstruct {
    /// Unique ID of the thing.
    pub id: String,
    /// The name of the thing that we can use in docker commands.
    pub name: String,
    /// Indicate whether the thing was created outside the test,
    /// or it can be destroyed when it goes out of scope.
    pub keep: bool,
}

/// Allocated (inclusive) range we can use to expose containers' ports on the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerPortRange {
    pub from: u32,
    pub to: u32,
}

/// Mapping ports assuming a 100 size ranges.
///
/// The ports on the host are assigned so that they end with the same number as the internal one,
/// which is hopefully a little bit intuitive for anyone who is familiar with the default values.
impl DockerPortRange {
    /// Mapping the internal 26655 port to the host.
    pub fn resolver_p2p_host_port(&self) -> u32 {
        self.from + 55
    }

    /// Mapping the internal 26656 port to the host.
    pub fn cometbft_p2p_host_port(&self) -> u32 {
        self.from + 56
    }

    /// Mapping the internal 26657 port to the host.
    pub fn cometbft_rpc_host_port(&self) -> u32 {
        self.from + 57
    }

    /// Mapping the internal 8445 port to the host.
    pub fn ethapi_rpc_host_port(&self) -> u32 {
        self.from + 45
    }

    /// Mapping the internal 9184 of fendermint to the host.
    pub fn fendermint_metrics_host_port(&self) -> u32 {
        self.from + 84
    }
}

/// State of the materializer that it persists, so that it can resume operations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockerMaterializerState {
    /// Port ranges ever allocated by this materializer.
    port_ranges: BTreeMap<NodeName, DockerPortRange>,
}

pub struct DockerMaterializer {
    dir: PathBuf,
    rng: StdRng,
    docker: bollard::Docker,
    drop_handle: dropper::DropHandle,
    drop_chute: dropper::DropChute,
    drop_policy: dropper::DropPolicy,
    state: DockerMaterializerState,
}

impl DockerMaterializer {
    /// Create a materializer with a directory where all the
    /// testnets can live next to each other.
    pub fn new(dir: &Path, seed: u64) -> anyhow::Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("failed to connect to Docker")?;

        // Create a runtime for the execution of drop tasks.
        let (drop_handle, drop_chute) = dropper::start(docker.clone());

        // Read in the state if it exists, otherwise create a default one.
        let state = import_json(dir.join(STATE_JSON_FILE_NAME))
            .context("failed to read state")?
            .unwrap_or_default();

        let m = Self {
            dir: dir.into(),
            rng: StdRng::seed_from_u64(seed),
            docker,
            drop_handle,
            drop_chute,
            state,
            drop_policy: DropPolicy::default(),
        };

        m.save_state().context("failed to save state")?;
        m.export_scripts().context("failed to export scripts")?;

        Ok(m)
    }

    pub fn with_policy(mut self, policy: DropPolicy) -> Self {
        self.drop_policy = policy;
        self
    }

    /// Remove all traces of a testnet.
    pub async fn remove(&mut self, testnet_name: &TestnetName) -> anyhow::Result<()> {
        let testnet = testnet_name.path_string();

        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec![format!("testnet={}", testnet)]);

        let containers: Vec<ContainerSummary> = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
            .context("failed to list docker containers")?;

        let ids = containers.into_iter().filter_map(|c| c.id);

        for id in ids {
            eprintln!("removing docker container {id}");
            self.docker
                .remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: true,
                        ..Default::default()
                    }),
                )
                .await
                .with_context(|| format!("failed to remove container {id}"))?;
        }

        let mut filters = HashMap::new();
        filters.insert("name".to_string(), vec![testnet]);

        let networks: Vec<Network> = self
            .docker
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .context("failed to list networks")?;

        let ids = networks.into_iter().filter_map(|n| n.id);

        for id in ids {
            eprintln!("removing docker network {id}");
            self.docker
                .remove_network(&id)
                .await
                .context("failed to remove network")?;
        }

        let dir = self.dir.join(testnet_name.path());
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            if !e.to_string().contains("No such file") {
                bail!(
                    "failed to remove testnet directory {}: {e:?}",
                    dir.to_string_lossy()
                );
            }
        };

        Ok(())
    }

    /// Replace the dropper with a new one and return the existing one so that we can await all the drop tasks being completed.
    pub fn take_dropper(&mut self) -> DropHandle {
        let (mut drop_handle, mut drop_chute) = dropper::start(self.docker.clone());
        std::mem::swap(&mut drop_handle, &mut self.drop_handle);
        std::mem::swap(&mut drop_chute, &mut self.drop_chute);
        // By dropping the `drop_chute` the only the existing docker constructs will keep a reference to it.
        // The caller can decide when it's time to wait on the handle, when the testnet have been dropped.
        drop_handle
    }

    /// Path to a directory based on a resource name.
    fn path<T: AsRef<ResourceName>>(&self, name: T) -> PathBuf {
        let name: &ResourceName = name.as_ref();
        self.dir.join(&name.0)
    }

    /// Path where the state of the materializer is saved.
    fn state_path(&self) -> PathBuf {
        self.dir.join(STATE_JSON_FILE_NAME)
    }

    /// Directory where scripts are exported, to be mounted into containers.
    fn scripts_dir(&self) -> PathBuf {
        self.dir.join("scripts")
    }

    /// Export scripts that need to be mounted.
    fn export_scripts(&self) -> anyhow::Result<()> {
        let scripts_dir = self.scripts_dir();
        export_script(scripts_dir.join("docker-entry.sh"), DOCKER_ENTRY_SCRIPT)?;
        Ok(())
    }

    /// Update the config file of the `ipc-cli` in a given testnet.
    fn update_ipc_cli_config<F, T>(&mut self, testnet_name: &TestnetName, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut IpcCliConfig) -> T,
    {
        let file_name = self.ipc_dir(testnet_name).join("config.toml");

        let mut config = if !file_name.exists() {
            IpcCliConfig {
                keystore_path: Some("~/.ipc".to_string()),
                subnets: Default::default(),
            }
        } else {
            IpcCliConfig::from_file(&file_name).context("failed to read ipc-cli config")?
        };

        let value = f(&mut config);

        let config_toml =
            toml::to_string_pretty(&config).context("failed to serialize ipc-cli config")?;

        export_file(&file_name, config_toml).context("failed to write ipc-cli config")?;

        Ok(value)
    }

    /// Update the state, save it to JSON, then return whatever value the update returns.
    fn update_state<F, T>(&mut self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut DockerMaterializerState) -> T,
    {
        let value = f(&mut self.state);
        self.save_state()?;
        Ok(value)
    }

    /// Write the state to a JSON file.
    fn save_state(&self) -> anyhow::Result<()> {
        export_json(self.state_path(), &self.state).context("failed to export state")
    }

    /// Return an existing genesis by parsing it from the `genesis.json` of the subnet,
    /// or create a new one and export it.
    fn get_or_create_genesis<F>(
        &self,
        subnet_name: &SubnetName,
        make_genesis: F,
    ) -> anyhow::Result<DefaultGenesis>
    where
        F: FnOnce() -> anyhow::Result<Genesis>,
    {
        let subnet_path = self.path(subnet_name);
        let genesis_path = subnet_path.join("genesis.json");

        let genesis = match import_json(&genesis_path).context("failed to read genesis")? {
            Some(genesis) => genesis,
            None => {
                let genesis = make_genesis().context("failed to make genesis")?;
                export_json(&genesis_path, &genesis).context("failed to export genesis")?;
                genesis
            }
        };

        Ok(DefaultGenesis {
            name: subnet_name.clone(),
            genesis,
            path: genesis_path,
        })
    }

    /// Pick a range for a container. Remember the choice so that we can recreate
    /// this materializer in a test and allocate more if needed without clashes.
    fn port_range(&mut self, node_name: &NodeName) -> anyhow::Result<DockerPortRange> {
        if let Some(range) = self.state.port_ranges.get(node_name) {
            return Ok(range.clone());
        }
        // Currently the range allocations are not dropped from the materializer,
        // so the length can be used to derive the next available port. Otherwise
        // we could loop through to find an unused slot.
        let node_count = self.state.port_ranges.len() as u32;
        let from = PORT_RANGE_START + PORT_RANGE_SIZE * node_count;
        let to = from + PORT_RANGE_SIZE;
        let range = DockerPortRange { from, to };
        self.update_state(|s| s.port_ranges.insert(node_name.clone(), range.clone()))?;
        Ok(range)
    }

    fn ipc_dir(&self, testnet_name: &TestnetName) -> PathBuf {
        self.path(testnet_name).join("ipc")
    }

    fn accounts_dir(&self, testnet_name: &TestnetName) -> PathBuf {
        self.path(testnet_name).join("accounts")
    }

    /// Create an instance of an `fendermint` command runner.
    fn fendermint_cli_runner(
        &self,
        subnet_name: &SubnetName,
        network_name: Option<&NetworkName>,
    ) -> anyhow::Result<DockerRunner<CliName>> {
        let subnet_dir = self.path(subnet_name);
        // Use the owner of the directory for the container, so we don't get permission issues.
        let user = user_id(&subnet_dir)?;
        // Mount the subnet so we can create files there
        let volumes = vec![(subnet_dir, "/fendermint/subnet")];

        let cli_name = subnet_name.cli("fendermint");

        let runner = DockerRunner::new(
            self.docker.clone(),
            self.drop_chute.clone(),
            self.drop_policy.clone(),
            cli_name,
            user,
            FENDERMINT_IMAGE,
            volumes,
            network_name.cloned(),
        );

        Ok(runner)
    }

    /// Create an instance of an `ipc-cli` command runner.
    fn ipc_cli_runner(
        &self,
        testnet_name: &TestnetName,
        network_name: Option<&NetworkName>,
    ) -> anyhow::Result<DockerRunner<CliName>> {
        // Create a directory to hold the wallet.
        let ipc_dir = self.ipc_dir(testnet_name);
        let accounts_dir = self.accounts_dir(testnet_name);
        // Create a `~/.ipc` directory, as expected by default by the `ipc-cli`.
        std::fs::create_dir_all(&ipc_dir).context("failed to create .ipc dir")?;
        // Use the owner of the directory for the container, so we don't get permission issues.
        let user = user_id(&ipc_dir)?;
        // Mount the `~/.ipc` directory and all the keys to be imported.
        let volumes = vec![
            (ipc_dir, "/fendermint/.ipc"),
            (accounts_dir, "/fendermint/accounts"),
        ];

        let cli_name = testnet_name.root().cli("ipc");

        let runner = DockerRunner::new(
            self.docker.clone(),
            self.drop_chute.clone(),
            self.drop_policy.clone(),
            cli_name,
            user,
            FENDERMINT_IMAGE,
            volumes,
            network_name.cloned(),
        );

        Ok(runner)
    }

    /// Import the private key of an account into the `ipc-cli` wallet.
    async fn ipc_cli_wallet_import(
        runner: &DockerRunner<CliName>,
        account: &DefaultAccount,
    ) -> anyhow::Result<()> {
        let account_id = account.account_id();
        let account_id: &str = account_id.as_ref();

        let cmd = format!(
            "ipc-cli wallet import \
                --wallet-type evm \
                --path /fendermint/accounts/{account_id}/secret.hex \
                "
        );

        // TODO: It would be nice to skip if already imported, but not crucial.
        runner
            .run_cmd(&cmd)
            .await
            .context("failed to import wallet")?;

        Ok(())
    }

    /// Add the subnet to the `config.toml` of the `ipc-cli`.
    fn ipc_cli_config_add_subnet(
        &mut self,
        submit_config: &SubmitConfig<DockerMaterials>,
    ) -> anyhow::Result<()> {
        let testnet_name = submit_config.subnet.name.testnet();
        let subnet_id = submit_config.subnet.subnet_id.clone();

        // Find a node to which the `ipc-cli` can connect to create the subnet.
        // Using the internal HTTP address, assumign that the dockerized `ipc-cli`
        // will always mount the config file and talk to the nodes within the docker network.
        let url: Url = submit_config
            .nodes
            .iter()
            .filter_map(|tc| match tc {
                TargetConfig::External(url) => Some(url.clone()),
                TargetConfig::Internal(node) => node.internal_ethapi_http_endpoint(),
            })
            .next()
            .ok_or_else(|| anyhow!("there has to be some nodes with eth API enabled"))?;

        // Create a `config.toml`` file for the `ipc-cli` based on the deployment of the parent.
        self.update_ipc_cli_config(&testnet_name, |config| {
            config.add_subnet(IpcCliSubnet {
                id: subnet_id,
                config: IpcCliSubnetConfig::Fevm(EVMSubnet {
                    provider_http: url,
                    provider_timeout: Some(Duration::from_secs(30)),
                    auth_token: None,
                    registry_addr: submit_config.deployment.registry.into(),
                    gateway_addr: submit_config.deployment.gateway.into(),
                }),
            })
        })
        .context("failed to update CLI config")?;

        Ok(())
    }

    /// Run some kind of command with the `ipc-cli` that needs to be executed as
    /// transaction by an account on a given subnet.
    async fn ipc_cli_run_cmd<'a>(
        &mut self,
        submit_config: &SubmitConfig<'a, DockerMaterials>,
        account: &DefaultAccount,
        cmd: String,
    ) -> anyhow::Result<Vec<String>> {
        // Make sure the config file exists before trying to run any commands.
        self.ipc_cli_config_add_subnet(submit_config)?;

        let submit_node = submit_config
            .nodes
            .iter()
            .filter_map(|tc| match tc {
                TargetConfig::Internal(node) => Some(node),
                TargetConfig::External(_) => None,
            })
            .next();

        let runner = self.ipc_cli_runner(
            &submit_config.subnet.name.testnet(),
            submit_node.map(|n| n.network_name()),
        )?;

        // Make sure the account we run the command with exists in the wallet.
        Self::ipc_cli_wallet_import(&runner, account).await?;

        let logs = runner
            .run_cmd(&cmd)
            .await
            .context("failed to run ipc-cli command")?;

        Ok(logs)
    }

    fn reference_path(&self, sn: &SubnetName, rh: &ResourceHash) -> PathBuf {
        self.path(sn.testnet()).join("refs").join(hex::encode(rh.0))
    }

    fn has_reference(&self, sn: &SubnetName, reference: &Option<ResourceHash>) -> bool {
        reference
            .as_ref()
            .map(|rh| self.reference_path(sn, rh).exists())
            .unwrap_or_default()
    }

    fn add_reference(
        &self,
        sn: &SubnetName,
        reference: &Option<ResourceHash>,
    ) -> anyhow::Result<()> {
        if let Some(ref rh) = reference {
            export_file(self.reference_path(sn, rh), "").context("failed to write reference")
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl Materializer<DockerMaterials> for DockerMaterializer {
    async fn create_network(
        &mut self,
        testnet_name: &TestnetName,
    ) -> anyhow::Result<<DockerMaterials as Materials>::Network> {
        DockerNetwork::get_or_create(
            self.docker.clone(),
            self.drop_chute.clone(),
            testnet_name.clone(),
            &self.drop_policy,
        )
        .await
    }

    /// Create a new key-value pair, or return an existing one.
    fn create_account(
        &mut self,
        account_name: &crate::AccountName,
    ) -> anyhow::Result<DefaultAccount> {
        DefaultAccount::get_or_create(&mut self.rng, &self.dir, account_name)
    }

    async fn fund_from_faucet<'s, 'a>(
        &'s mut self,
        account: &'a DefaultAccount,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        todo!("use curl or something to trigger the faucet")
    }

    async fn new_deployment<'s, 'a>(
        &'s mut self,
        subnet_name: &SubnetName,
        deployer: &'a DefaultAccount,
        urls: Vec<Url>,
    ) -> anyhow::Result<DefaultDeployment>
    where
        's: 'a,
    {
        todo!("use the deploy scripts to create a new IPC stack on L1")
    }

    fn existing_deployment(
        &mut self,
        subnet_name: &SubnetName,
        gateway: H160,
        registry: H160,
    ) -> anyhow::Result<DefaultDeployment> {
        Ok(DefaultDeployment {
            name: subnet_name.clone(),
            gateway: EthAddress::from(gateway),
            registry: EthAddress::from(registry),
        })
    }

    fn default_deployment(
        &mut self,
        subnet_name: &SubnetName,
    ) -> anyhow::Result<DefaultDeployment> {
        Ok(DefaultDeployment::builtin(subnet_name.clone()))
    }

    /// Check if a genesis file already exists. If so, parse it, otherwise
    /// create an in-memory representation of a genesis file and export it.
    fn create_root_genesis<'a>(
        &mut self,
        subnet_name: &SubnetName,
        validators: BTreeMap<&'a DefaultAccount, Collateral>,
        balances: BTreeMap<&'a DefaultAccount, Balance>,
    ) -> anyhow::Result<DefaultGenesis> {
        self.get_or_create_genesis(subnet_name, || {
            let chain_name = subnet_name.path_string();
            let chain_id = chainid::from_str_hashed(&chain_name)?;
            // TODO: Some of these hardcoded values can go into the manifest.
            let genesis = Genesis {
                chain_name,
                timestamp: Timestamp::current(),
                network_version: NetworkVersion::V21,
                base_fee: TokenAmount::zero(),
                power_scale: 3,
                validators: validators
                    .into_iter()
                    .map(|(v, c)| Validator {
                        public_key: ValidatorKey(*v.public_key()),
                        power: c,
                    })
                    .collect(),
                accounts: balances
                    .into_iter()
                    .map(|(a, b)| Actor {
                        meta: ActorMeta::Account(Account {
                            owner: SignerAddr(a.fvm_addr()),
                        }),
                        balance: b.0,
                    })
                    .collect(),
                eam_permission_mode: fendermint_vm_genesis::PermissionMode::Unrestricted,
                ipc: Some(IpcParams {
                    gateway: GatewayParams {
                        subnet_id: SubnetID::new_root(chain_id.into()),
                        // TODO: The gateway constructor doesn't allow 0 bottom-up-checkpoint-period even on the rootnet!
                        bottom_up_check_period: 1,
                        majority_percentage: 67,
                        active_validators_limit: 100,
                    },
                }),
            };
            Ok(genesis)
        })
    }

    fn create_root_subnet(
        &mut self,
        subnet_name: &SubnetName,
        params: Either<ChainID, &DefaultGenesis>,
    ) -> anyhow::Result<DefaultSubnet> {
        let subnet_id = match params {
            Either::Left(id) => SubnetID::new_root(id.into()),
            Either::Right(g) => {
                let ipc = g
                    .genesis
                    .ipc
                    .as_ref()
                    .ok_or_else(|| anyhow!("IPC configuration missing from genesis"))?;

                ipc.gateway.subnet_id.clone()
            }
        };

        Ok(DefaultSubnet {
            name: subnet_name.clone(),
            subnet_id,
        })
    }

    /// Get or create all docker containers that constitute to a Node.
    async fn create_node<'s, 'a>(
        &'s mut self,
        node_name: &NodeName,
        node_config: &NodeConfig<'a, DockerMaterials>,
    ) -> anyhow::Result<DockerNode>
    where
        's: 'a,
    {
        // Pick a port range on the host.
        let port_range = self
            .port_range(node_name)
            .context("failed to pick port range")?;

        // We could write a (shared) docker-compose.yaml file and .env file per node,
        // however the `bollard` library doesn't support docker-compose, so different
        // techniques would need to be used. Alternatively we can just use `Docker`
        // and run three different containers.
        DockerNode::get_or_create(
            &self.dir,
            self.docker.clone(),
            self.drop_chute.clone(),
            &self.drop_policy,
            node_name,
            node_config,
            port_range,
        )
        .await
        .context("failed to create node")
    }

    async fn start_node<'s, 'a>(
        &'s mut self,
        node: &'a DockerNode,
        seed_nodes: &'a [&'a DockerNode],
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        // Overwrite the env file which has seed addresses, then start the node (unless it's already running).
        node.start(seed_nodes).await?;
        node.wait_for_started(*STARTUP_TIMEOUT).await?;
        // Trying to avoid `Tendermint RPC error: server returned malformatted JSON (no 'result' or 'error')` on first subnet creation attempt.
        tokio::time::sleep(Duration::from_secs(5)).await;
        Ok(())
    }

    async fn create_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, DockerMaterials>,
        subnet_name: &SubnetName,
        subnet_config: &SubnetConfig<'a, DockerMaterials>,
    ) -> anyhow::Result<DefaultSubnet>
    where
        's: 'a,
    {
        let subnet_dir = self.path(subnet_name);
        let subnet_id_file = subnet_dir.join("subnet-id");

        // Check if we have already created the subnet.
        if subnet_id_file.exists() {
            let subnet_id = std::fs::read_to_string(&subnet_id_file)
                .context("failed to read subnet ID from file")?;

            let subnet_id = SubnetID::from_str(&subnet_id).with_context(|| {
                format!(
                    "failed to parse subnet ID in {}: {}",
                    subnet_id_file.to_string_lossy(),
                    subnet_id
                )
            })?;

            let subnet = DefaultSubnet {
                subnet_id,
                name: subnet_name.clone(),
            };

            return Ok(subnet);
        }

        // TODO: Move --permission-mode to the config
        // TODO: Move --supply-source-kind to the config
        let cmd = format!(
            "ipc-cli subnet create \
                --parent {} \
                --from {:?} \
                --min-validators {} \
                --min-validator-stake {} \
                --bottomup-check-period {} \
                --permission-mode collateral \
                --supply-source-kind native \
                ",
            parent_submit_config.subnet.subnet_id,
            subnet_config.creator.eth_addr(),
            subnet_config.min_validators,
            TokenAmount::from_nano(1), // The minimum for native mode that the CLI parses
            subnet_config.bottom_up_checkpoint.period
        );

        // Now run the command and capture the output.
        let logs = self
            .ipc_cli_run_cmd(parent_submit_config, subnet_config.creator, cmd)
            .await
            .context("failed to create subnet")?;

        // Parse the subnet ID from the command output.
        let subnet_id = logs
            .last()
            .and_then(find_subnet_id)
            .ok_or_else(|| anyhow!("cannot find a subnet ID in the logs"))?
            .context("failed to parse subnet ID")?;

        export_file(subnet_id_file, subnet_id.to_string()).context("failed to export subnet ID")?;

        Ok(DefaultSubnet {
            name: subnet_name.clone(),
            subnet_id,
        })
    }

    async fn fund_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, DockerMaterials>,
        account: &'a DefaultAccount,
        subnet: &'a DefaultSubnet,
        amount: fvm_shared::econ::TokenAmount,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        if self.has_reference(&subnet.name, &reference) {
            return Ok(());
        }

        let cmd = format!(
            "ipc-cli cross-msg fund \
                --subnet {} \
                --from {:?} \
                --to {:?} \
                {} \
            ",
            subnet.subnet_id,
            account.eth_addr(),
            account.eth_addr(),
            amount
        );

        let logs = self
            .ipc_cli_run_cmd(parent_submit_config, account, cmd)
            .await
            .context("failed to fund subnet")?;

        self.add_reference(&subnet.name, &reference)
    }

    async fn join_subnet<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, DockerMaterials>,
        account: &'a DefaultAccount,
        subnet: &'a DefaultSubnet,
        collateral: fendermint_vm_genesis::Collateral,
        balance: Balance,
        reference: Option<ResourceHash>,
    ) -> anyhow::Result<()>
    where
        's: 'a,
    {
        if self.has_reference(&subnet.name, &reference) {
            return Ok(());
        }

        let cmd = format!(
            "ipc-cli subnet join \
                --subnet {} \
                --from {:?} \
                --collateral {} \
                --initial-balance {} \
            ",
            subnet.subnet_id,
            account.eth_addr(),
            collateral.0,
            balance.0
        );

        self.ipc_cli_run_cmd(parent_submit_config, account, cmd)
            .await
            .context("failed to join subnet")?;

        self.add_reference(&subnet.name, &reference)
    }

    async fn create_subnet_genesis<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, DockerMaterials>,
        subnet: &'a DefaultSubnet,
    ) -> anyhow::Result<DefaultGenesis>
    where
        's: 'a,
    {
        let network_name =
            parent_submit_config.find_node(|n| Some(n.network_name().clone()), |_| None);

        let parent_url: Url = parent_submit_config
            .find_node(|n| n.internal_ethapi_http_endpoint(), |u| Some(u.clone()))
            .ok_or_else(|| anyhow!("there has to be some nodes with eth API enabled"))?;

        // TODO: Move --base-fee to config
        // TODO: Move --power-scale to config
        let cmd = format!(
            "genesis \
                --genesis-file /fendermint/subnet/genesis.json \
                ipc from-parent \
                    --subnet-id {} \
                    --parent-endpoint {} \
                    --parent-gateway {:?} \
                    --parent-registry {:?} \
                    --base-fee {} \
                    --power-scale {} \
                ",
            subnet.subnet_id,
            parent_url,
            parent_submit_config.deployment.gateway,
            parent_submit_config.deployment.registry,
            TokenAmount::zero().atto(),
            9, // to work with nanoFIL
        );

        let runner = self.fendermint_cli_runner(&subnet.name, network_name.as_ref())?;

        runner
            .run_cmd(&cmd)
            .await
            .context("failed to fetch genesis from parent")?;

        let genesis_path = self.path(&subnet.name).join("genesis.json");

        let genesis = import_json::<Genesis>(&genesis_path)
            .context("failed to read genesis.json")?
            .ok_or_else(|| anyhow!("genesis.json doesn't exist after fetching from parent"))?;

        let genesis = DefaultGenesis {
            name: subnet.name.clone(),
            genesis,
            path: genesis_path,
        };

        Ok(genesis)
    }

    async fn create_relayer<'s, 'a>(
        &'s mut self,
        parent_submit_config: &SubmitConfig<'a, DockerMaterials>,
        relayer_name: &RelayerName,
        relayer_config: RelayerConfig<'a, DockerMaterials>,
    ) -> anyhow::Result<DockerRelayer>
    where
        's: 'a,
    {
        let network_name = relayer_config
            .follow_config
            .find_node(|n| Some(n.network_name().clone()), |_| None);

        // Add the parent subnet to the config.toml
        self.ipc_cli_config_add_subnet(parent_submit_config)?;

        // Add the child subnet to the config.toml
        self.ipc_cli_config_add_subnet(relayer_config.follow_config)?;

        // Add the submitter to the IPC wallet
        Self::ipc_cli_wallet_import(
            &self.ipc_cli_runner(&parent_submit_config.subnet.name.testnet(), None)?,
            relayer_config.submitter,
        )
        .await?;

        // Create the relayer
        let relayer = DockerRelayer::get_or_create(
            &self.dir,
            self.docker.clone(),
            self.drop_chute.clone(),
            &self.drop_policy,
            relayer_name,
            relayer_config.follow_config.subnet,
            relayer_config.submitter,
            network_name,
            relayer_config.env,
        )
        .await?;

        // Start the relayer
        relayer.start().await?;

        Ok(relayer)
    }
}

/// The `ipc-cli` puts the output in a human readable log instead of printing JSON.
fn find_subnet_id(log: impl AsRef<str>) -> Option<Result<SubnetID, ipc_api::error::Error>> {
    lazy_static! {
        static ref SUBNET_ID_RE: Regex =
            Regex::new(r"(/r\d+(/[tf]410[0-9a-z]{40})+)").expect("subnet regex parses");
    }
    SUBNET_ID_RE
        .find(log.as_ref())
        .map(|m| m.as_str())
        .map(SubnetID::from_str)
}

/// The current address network needs to be set on the containers to match the addresses we created.
fn current_network() -> &'static str {
    match fvm_shared::address::current_network() {
        fvm_shared::address::Network::Mainnet => "mainnet",
        fvm_shared::address::Network::Testnet => "testnet",
    }
}

/// Get the user ID we can use with docker to have the same file permissions
/// as some file or directory on the file system, so that files created by a
/// container can be owned by the same user, rather than root.
fn user_id(path: impl AsRef<Path>) -> anyhow::Result<u32> {
    Ok(path.as_ref().metadata()?.uid())
}

#[cfg(test)]
mod tests {
    use fendermint_vm_actor_interface::ipc;
    use fvm_shared::address::Address;
    use ipc_api::subnet_id::SubnetID;
    use ipc_provider::config::subnet::{
        EVMSubnet, Subnet as IpcCliSubnet, SubnetConfig as IpcCliSubnetConfig,
    };
    use ipc_provider::config::Config as IpcCliConfig;
    use std::str::FromStr;
    use std::time::Duration;

    use super::find_subnet_id;

    #[test]
    fn test_ipc_cli_config_toml_roundtrip() {
        let mut config0 = IpcCliConfig {
            keystore_path: Some("~/.ipc".to_string()),
            subnets: Default::default(),
        };

        config0.add_subnet(IpcCliSubnet {
            id: SubnetID::new_root(12345),
            config: IpcCliSubnetConfig::Fevm(EVMSubnet {
                provider_http: url::Url::parse("http://example.net").unwrap(),
                provider_timeout: Some(Duration::from_secs(30)),
                auth_token: None,
                registry_addr: ipc::SUBNETREGISTRY_ACTOR_ADDR,
                gateway_addr: ipc::GATEWAY_ACTOR_ADDR,
            }),
        });

        let config_toml = toml::to_string_pretty(&config0).expect("failed to serialize");
        eprintln!("{config_toml}");

        let config1 = IpcCliConfig::from_toml_str(&config_toml).expect("failed to deserialize");

        assert_eq!(config0, config1);
    }

    #[test]
    fn test_parse_subnet_id_from_log() {
        let example = "[2024-03-05T15:10:01Z INFO  ipc_cli::commands::subnet::create] created subnet actor with id: /r314159/f410fu6ua642sypnlukccd3gaizwhonk5kwlpml6r3pa";
        let expected = SubnetID::new_from_parent(
            &SubnetID::new_root(314159),
            Address::from_str("f410fu6ua642sypnlukccd3gaizwhonk5kwlpml6r3pa").unwrap(),
        );
        assert_eq!(find_subnet_id(example), Some(Ok(expected)));
    }

    #[test]
    fn test_parse_subnet_id_from_log_wrong_network() {
        let example = "[2024-03-05T15:10:01Z INFO  ipc_cli::commands::subnet::create] created subnet actor with id: /r314159/t410fu6ua642sypnlukccd3gaizwhonk5kwlpml6r3pa";
        find_subnet_id(example)
            .expect("should find the subnet ID")
            .expect_err("should fail to parse t410 address");
    }
}
