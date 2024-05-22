// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::{BTreeSet, HashMap};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use ethers::abi::Tokenize;
use ethers::core::types as et;
use fendermint_actor_eam::PermissionModeParams;
use fendermint_eth_hardhat::{Hardhat, FQN};
use fendermint_vm_actor_interface::diamond::{EthContract, EthContractMap};
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_actor_interface::ipc::IPC_CONTRACTS;
use fendermint_vm_actor_interface::{
    account, burntfunds, chainmetadata, cron, eam, init, ipc, reward, system, EMPTY_ARR,
};
use fendermint_vm_core::{chainid, Timestamp};
use fendermint_vm_genesis::{ActorMeta, Genesis, Power, PowerScale, Validator};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::chainid::ChainID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::version::NetworkVersion;
use ipc_actors_abis::i_diamond::FacetCut;
use num_traits::Zero;

use crate::GenesisInterpreter;

use super::state::FvmGenesisState;
use super::FvmMessageInterpreter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FvmGenesisOutput {
    pub chain_id: ChainID,
    pub timestamp: Timestamp,
    pub network_version: NetworkVersion,
    pub base_fee: TokenAmount,
    pub power_scale: PowerScale,
    pub circ_supply: TokenAmount,
    pub validators: Vec<Validator<Power>>,
}

#[async_trait]
impl<DB, TC> GenesisInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + 'static + Send + Sync + Clone,
    TC: Send + Sync + 'static,
{
    type State = FvmGenesisState<DB>;
    type Genesis = Genesis;
    type Output = FvmGenesisOutput;

    /// Initialize actor states from the Genesis spec.
    ///
    /// This method doesn't create all builtin Filecoin actors,
    /// it leaves out the ones specific to file storage.
    ///
    /// The ones included are:
    /// * system
    /// * init
    /// * cron
    /// * EAM
    /// * burnt funds
    /// * rewards (placeholder)
    /// * accounts
    /// * IPC
    ///
    /// TODO:
    /// * faucet?
    ///
    /// See genesis initialization in:
    /// * [Lotus](https://github.com/filecoin-project/lotus/blob/v1.20.4/chain/gen/genesis/genesis.go)
    /// * [ref-fvm tester](https://github.com/filecoin-project/ref-fvm/blob/fvm%40v3.1.0/testing/integration/src/tester.rs#L99-L103)
    /// * [fvm-workbench](https://github.com/anorth/fvm-workbench/blob/67219b3fd0b5654d54f722ab5acea6ec0abb2edc/builtin/src/genesis.rs)
    async fn init(
        &self,
        mut state: Self::State,
        genesis: Self::Genesis,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        // Log the genesis in JSON format, hopefully it's not enormous.
        tracing::debug!(genesis = serde_json::to_string(&genesis)?, "init");

        // NOTE: We could consider adding the chain ID to the interpreter
        //       and rejecting genesis if it doesn't match the expectation,
        //       but the Tendermint genesis file also has this field, and
        //       presumably Tendermint checks that its peers have the same.
        let chain_id = chainid::from_str_hashed(&genesis.chain_name)?;

        // Convert validators to CometBFT power scale.
        let validators = genesis
            .validators
            .iter()
            .cloned()
            .map(|vc| vc.map_power(|c| c.into_power(genesis.power_scale)))
            .collect();

        // Currently we just pass them back as they are, but later we should
        // store them in the IPC actors; or in case of a snapshot restore them
        // from the state.
        let out = FvmGenesisOutput {
            chain_id,
            timestamp: genesis.timestamp,
            network_version: genesis.network_version,
            circ_supply: circ_supply(&genesis),
            base_fee: genesis.base_fee,
            power_scale: genesis.power_scale,
            validators,
        };

        // STAGE 0: Declare the built-in EVM contracts we'll have to deploy.

        // Pre-defined IDs for top-level Ethereum contracts.
        let mut eth_builtin_ids = BTreeSet::new();
        let mut eth_root_contracts = Vec::new();
        let mut eth_contracts = EthContractMap::default();

        // Only allocate IDs if the contracts are deployed.
        if genesis.ipc.is_some() {
            eth_contracts.extend(IPC_CONTRACTS.clone());
        }

        eth_builtin_ids.extend(eth_contracts.values().map(|c| c.actor_id));
        eth_root_contracts.extend(eth_contracts.keys());
        eth_root_contracts.extend(
            eth_contracts
                .values()
                .flat_map(|c| c.facets.iter().map(|f| f.name)),
        );
        // Collect dependencies of the main IPC actors.
        let mut eth_libs = self
            .contracts
            .dependencies(
                &eth_root_contracts
                    .iter()
                    .map(|n| (contract_src(n), *n))
                    .collect::<Vec<_>>(),
            )
            .context("failed to collect EVM contract dependencies")?;

        // Only keep library dependencies, not contracts with constructors.
        eth_libs.retain(|(_, d)| !eth_contracts.contains_key(d.as_str()));

        // STAGE 1: First we initialize native built-in actors.

        // System actor
        state
            .create_builtin_actor(
                system::SYSTEM_ACTOR_CODE_ID,
                system::SYSTEM_ACTOR_ID,
                &system::State {
                    builtin_actors: state.manifest_data_cid,
                },
                TokenAmount::zero(),
                None,
            )
            .context("failed to create system actor")?;

        // Init actor
        let (init_state, addr_to_id) = init::State::new(
            state.store(),
            genesis.chain_name.clone(),
            &genesis.accounts,
            &eth_builtin_ids,
            eth_libs.len() as u64,
        )
        .context("failed to create init state")?;

        state
            .create_builtin_actor(
                init::INIT_ACTOR_CODE_ID,
                init::INIT_ACTOR_ID,
                &init_state,
                TokenAmount::zero(),
                None,
            )
            .context("failed to create init actor")?;

        // Cron actor
        state
            .create_builtin_actor(
                cron::CRON_ACTOR_CODE_ID,
                cron::CRON_ACTOR_ID,
                &cron::State {
                    entries: vec![], // TODO: Maybe with the IPC.
                },
                TokenAmount::zero(),
                None,
            )
            .context("failed to create cron actor")?;

        // Ethereum Account Manager (EAM) actor
        state
            .create_builtin_actor(
                eam::EAM_ACTOR_CODE_ID,
                eam::EAM_ACTOR_ID,
                &EMPTY_ARR,
                TokenAmount::zero(),
                None,
            )
            .context("failed to create EAM actor")?;

        // Burnt funds actor (it's just an account).
        state
            .create_builtin_actor(
                account::ACCOUNT_ACTOR_CODE_ID,
                burntfunds::BURNT_FUNDS_ACTOR_ID,
                &account::State {
                    address: burntfunds::BURNT_FUNDS_ACTOR_ADDR,
                },
                TokenAmount::zero(),
                None,
            )
            .context("failed to create burnt funds actor")?;

        // A placeholder for the reward actor, beause I don't think
        // using the one in the builtin actors library would be appropriate.
        // This effectively burns the miner rewards. Better than panicking.
        state
            .create_builtin_actor(
                account::ACCOUNT_ACTOR_CODE_ID,
                reward::REWARD_ACTOR_ID,
                &account::State {
                    address: reward::REWARD_ACTOR_ADDR,
                },
                TokenAmount::zero(),
                None,
            )
            .context("failed to create reward actor")?;

        // STAGE 1b: Then we initialize the in-repo custom actors.

        // Initialize the chain metadata actor which handles saving metadata about the chain
        // (e.g. block hashes) which we can query.
        let chainmetadata_state = fendermint_actor_chainmetadata::State::new(
            &state.store(),
            fendermint_actor_chainmetadata::DEFAULT_LOOKBACK_LEN,
        )?;
        state
            .create_custom_actor(
                fendermint_actor_chainmetadata::CHAINMETADATA_ACTOR_NAME,
                chainmetadata::CHAINMETADATA_ACTOR_ID,
                &chainmetadata_state,
                TokenAmount::zero(),
                None,
            )
            .context("failed to create chainmetadata actor")?;

        let eam_state = fendermint_actor_eam::State::new(
            state.store(),
            PermissionModeParams::from(genesis.eam_permission_mode),
        )?;
        state
            .replace_builtin_actor(
                eam::EAM_ACTOR_NAME,
                eam::EAM_ACTOR_ID,
                fendermint_actor_eam::IPC_EAM_ACTOR_NAME,
                &eam_state,
                TokenAmount::zero(),
                None,
            )
            .context("failed to replace built in eam actor")?;

        // STAGE 2: Create non-builtin accounts which do not have a fixed ID.

        // The next ID is going to be _after_ the accounts, which have already been assigned an ID by the `Init` actor.
        // The reason we aren't using the `init_state.next_id` is because that already accounted for the multisig accounts.
        let mut next_id = init::FIRST_NON_SINGLETON_ADDR + addr_to_id.len() as u64;

        for a in genesis.accounts {
            let balance = a.balance;
            match a.meta {
                ActorMeta::Account(acct) => {
                    state
                        .create_account_actor(acct, balance, &addr_to_id)
                        .context("failed to create account actor")?;
                }
                ActorMeta::Multisig(ms) => {
                    state
                        .create_multisig_actor(ms, balance, &addr_to_id, next_id)
                        .context("failed to create multisig actor")?;
                    next_id += 1;
                }
            }
        }

        // STAGE 3: Initialize the FVM and create built-in FEVM actors.

        state
            .init_exec_state(
                out.timestamp,
                out.network_version,
                out.base_fee.clone(),
                out.circ_supply.clone(),
                out.chain_id.into(),
                out.power_scale,
            )
            .context("failed to init exec state")?;

        let mut deployer = ContractDeployer::<DB>::new(&self.contracts, &eth_contracts);

        // Deploy Ethereum libraries.
        for (lib_src, lib_name) in eth_libs {
            deployer.deploy_library(&mut state, &mut next_id, lib_src, &lib_name)?;
        }

        if let Some(ipc_params) = genesis.ipc {
            // IPC Gateway actor.
            let gateway_addr = {
                use ipc::gateway::ConstructorParameters;

                let params = ConstructorParameters::new(ipc_params.gateway, genesis.validators)
                    .context("failed to create gateway constructor")?;

                let facets = deployer
                    .facets(ipc::gateway::CONTRACT_NAME)
                    .context("failed to collect gateway facets")?;

                deployer.deploy_contract(
                    &mut state,
                    ipc::gateway::CONTRACT_NAME,
                    (facets, params),
                )?
            };

            // IPC SubnetRegistry actor.
            {
                use ipc::registry::ConstructorParameters;

                let mut facets = deployer
                    .facets(ipc::registry::CONTRACT_NAME)
                    .context("failed to collect registry facets")?;

                let getter_facet = facets.remove(0);
                let manager_facet = facets.remove(0);
                let rewarder_facet = facets.remove(0);
                let checkpointer_facet = facets.remove(0);
                let pauser_facet = facets.remove(0);
                let diamond_loupe_facet = facets.remove(0);
                let diamond_cut_facet = facets.remove(0);
                let ownership_facet = facets.remove(0);

                debug_assert_eq!(facets.len(), 2, "SubnetRegistry has 2 facets of its own");

                let params = ConstructorParameters {
                    gateway: gateway_addr,
                    getter_facet: getter_facet.facet_address,
                    manager_facet: manager_facet.facet_address,
                    rewarder_facet: rewarder_facet.facet_address,
                    pauser_facet: pauser_facet.facet_address,
                    checkpointer_facet: checkpointer_facet.facet_address,
                    diamond_cut_facet: diamond_cut_facet.facet_address,
                    diamond_loupe_facet: diamond_loupe_facet.facet_address,
                    ownership_facet: ownership_facet.facet_address,
                    subnet_getter_selectors: getter_facet.function_selectors,
                    subnet_manager_selectors: manager_facet.function_selectors,
                    subnet_rewarder_selectors: rewarder_facet.function_selectors,
                    subnet_checkpointer_selectors: checkpointer_facet.function_selectors,
                    subnet_pauser_selectors: pauser_facet.function_selectors,
                    subnet_actor_diamond_cut_selectors: diamond_cut_facet.function_selectors,
                    subnet_actor_diamond_loupe_selectors: diamond_loupe_facet.function_selectors,
                    subnet_actor_ownership_selectors: ownership_facet.function_selectors,
                    creation_privileges: 0,
                };

                deployer.deploy_contract(
                    &mut state,
                    ipc::registry::CONTRACT_NAME,
                    (facets, params),
                )?;
            };
        }

        Ok((state, out))
    }
}

fn contract_src(name: &str) -> PathBuf {
    PathBuf::from(format!("{name}.sol"))
}

struct ContractDeployer<'a, DB> {
    hardhat: &'a Hardhat,
    top_contracts: &'a EthContractMap,
    // Assign dynamic ID addresses to libraries, but use fixed addresses for the top level contracts.
    lib_addrs: HashMap<FQN, et::Address>,
    phantom_db: PhantomData<DB>,
}

impl<'a, DB> ContractDeployer<'a, DB>
where
    DB: Blockstore + 'static + Send + Sync + Clone,
{
    pub fn new(hardhat: &'a Hardhat, top_contracts: &'a EthContractMap) -> Self {
        Self {
            hardhat,
            top_contracts,
            lib_addrs: Default::default(),
            phantom_db: PhantomData,
        }
    }

    /// Deploy a library contract with a dynamic ID and no constructor.
    pub fn deploy_library(
        &mut self,
        state: &mut FvmGenesisState<DB>,
        next_id: &mut u64,
        lib_src: impl AsRef<Path>,
        lib_name: &str,
    ) -> anyhow::Result<()> {
        let fqn = self.hardhat.fqn(lib_src.as_ref(), lib_name);

        let bytecode = self
            .hardhat
            .bytecode(&lib_src, lib_name, &self.lib_addrs)
            .with_context(|| format!("failed to load library bytecode {fqn}"))?;

        let eth_addr = state
            .create_evm_actor(*next_id, bytecode)
            .with_context(|| format!("failed to create library actor {fqn}"))?;

        let id_addr = et::Address::from(EthAddress::from_id(*next_id).0);
        let eth_addr = et::Address::from(eth_addr.0);

        tracing::info!(
            actor_id = next_id,
            ?eth_addr,
            ?id_addr,
            fqn,
            "deployed Ethereum library"
        );

        // We can use the masked ID here or the delegated address.
        // Maybe the masked ID is quicker because it doesn't need to be resolved.
        self.lib_addrs.insert(fqn, id_addr);

        *next_id += 1;

        Ok(())
    }

    /// Construct the bytecode of a top-level contract and deploy it with some constructor parameters.
    pub fn deploy_contract<T>(
        &self,
        state: &mut FvmGenesisState<DB>,
        contract_name: &str,
        constructor_params: T,
    ) -> anyhow::Result<et::Address>
    where
        T: Tokenize,
    {
        let contract = self.top_contract(contract_name)?;
        let contract_id = contract.actor_id;
        let contract_src = contract_src(contract_name);

        let bytecode = self
            .hardhat
            .bytecode(contract_src, contract_name, &self.lib_addrs)
            .with_context(|| format!("failed to load {contract_name} bytecode"))?;

        let eth_addr = state
            .create_evm_actor_with_cons(contract_id, &contract.abi, bytecode, constructor_params)
            .with_context(|| format!("failed to create {contract_name} actor"))?;

        let id_addr = et::Address::from(EthAddress::from_id(contract_id).0);
        let eth_addr = et::Address::from(eth_addr.0);

        tracing::info!(
            actor_id = contract_id,
            ?eth_addr,
            ?id_addr,
            contract_name,
            "deployed Ethereum contract"
        );

        // The Ethereum address is more usable inside the EVM than the ID address.
        Ok(eth_addr)
    }

    /// Collect Facet Cuts for the diamond pattern, where the facet address comes from already deployed library facets.
    pub fn facets(&self, contract_name: &str) -> anyhow::Result<Vec<FacetCut>> {
        let contract = self.top_contract(contract_name)?;
        let mut facet_cuts = Vec::new();

        for facet in contract.facets.iter() {
            let facet_name = facet.name;
            let facet_src = contract_src(facet_name);
            let facet_fqn = self.hardhat.fqn(&facet_src, facet_name);

            let facet_addr = self
                .lib_addrs
                .get(&facet_fqn)
                .ok_or_else(|| anyhow!("facet {facet_name} has not been deployed"))?;

            let method_sigs = facet
                .abi
                .functions()
                .filter(|f| f.signature() != "init(bytes)")
                .map(|f| f.short_signature())
                .collect();

            let facet_cut = FacetCut {
                facet_address: *facet_addr,
                action: 0, // Add
                function_selectors: method_sigs,
            };

            facet_cuts.push(facet_cut);
        }

        Ok(facet_cuts)
    }

    fn top_contract(&self, contract_name: &str) -> anyhow::Result<&EthContract> {
        self.top_contracts
            .get(contract_name)
            .ok_or_else(|| anyhow!("unknown top contract name: {contract_name}"))
    }
}

/// Sum of balances in the genesis accounts.
fn circ_supply(g: &Genesis) -> TokenAmount {
    g.accounts
        .iter()
        .fold(TokenAmount::zero(), |s, a| s + a.balance.clone())
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use cid::Cid;
    use fendermint_vm_genesis::{ipc::IpcParams, Genesis};
    use fvm::engine::MultiEngine;
    use quickcheck::Arbitrary;
    use tendermint_rpc::{MockClient, MockRequestMethodMatcher};

    use crate::{
        fvm::{
            bundle::{bundle_path, contracts_path, custom_actors_bundle_path},
            state::ipc::GatewayCaller,
            store::memory::MemoryBlockstore,
            upgrades::UpgradeScheduler,
            FvmMessageInterpreter,
        },
        GenesisInterpreter,
    };

    use super::FvmGenesisState;

    #[tokio::test]
    async fn load_genesis() {
        let genesis = make_genesis();
        let bundle = read_bundle();
        let custom_actors_bundle = read_custom_actors_bundle();
        let interpreter = make_interpreter();

        let multi_engine = Arc::new(MultiEngine::default());
        let store = MemoryBlockstore::new();

        let state = FvmGenesisState::new(store, multi_engine, &bundle, &custom_actors_bundle)
            .await
            .expect("failed to create state");

        let (mut state, out) = interpreter
            .init(state, genesis.clone())
            .await
            .expect("failed to create actors");

        assert_eq!(out.validators.len(), genesis.validators.len());

        // Try calling a method on the IPC Gateway.
        let exec_state = state.exec_state().expect("should be in exec stage");
        let caller = GatewayCaller::default();

        let period = caller
            .bottom_up_check_period(exec_state)
            .expect("error calling the gateway");

        assert_eq!(period, genesis.ipc.unwrap().gateway.bottom_up_check_period);

        let _state_root = state.commit().expect("failed to commit");
    }

    #[tokio::test]
    async fn load_genesis_deterministic() {
        let genesis = make_genesis();
        let bundle = read_bundle();
        let custom_actors_bundle = read_custom_actors_bundle();
        let interpreter = make_interpreter();
        let multi_engine = Arc::new(MultiEngine::default());

        // Create a couple of states and load the same thing.
        let mut outputs = Vec::new();
        for _ in 0..3 {
            let store = MemoryBlockstore::new();
            let state =
                FvmGenesisState::new(store, multi_engine.clone(), &bundle, &custom_actors_bundle)
                    .await
                    .expect("failed to create state");

            let (state, out) = interpreter
                .init(state, genesis.clone())
                .await
                .expect("failed to create actors");

            let state_root_hash = state.commit().expect("failed to commit");
            outputs.push((state_root_hash, out));
        }

        for out in &outputs[1..] {
            assert_eq!(out.0, outputs[0].0, "state root hash is different");
        }
    }

    // This is a sort of canary test, if it fails means something changed in the way we do genesis,
    // which is probably fine, but it's better to know about it, and if anybody doesn't get the same
    // then we might have some non-determinism.
    #[ignore] // I see a different value on CI than locally.
    #[tokio::test]
    async fn load_genesis_known() {
        let genesis_json = "{\"chain_name\":\"/r314159/f410fnfmitm2ww7oehhtbokf6wulhrr62sgq3sgqmenq\",\"timestamp\":1073250,\"network_version\":18,\"base_fee\":\"1000\",\"power_scale\":3,\"validators\":[{\"public_key\":\"BLX9ojqB+8Z26aMmKoCRb3Te6AnSU6zY8hPcf1X5Q69XCNaHVcRxzYO2xx7o/2vgdS7nkDTMRRbkDGzy+FYdAFc=\",\"power\":\"1000000000000000000\"},{\"public_key\":\"BFcOveVieknZiscWsfXa06aGbBkKeucBycd/w0N1QHlaZfa/5dJcH7D0hvcdfv3B2Rv1OPuxo1PkgsEbWegWKcA=\",\"power\":\"1000000000000000000\"},{\"public_key\":\"BEP30ykovfrQp3zo+JVRvDVL2emC+Ju1Kpox3zMVYZyFKvYt64qyN/HOVjridDrkEsnQU8BVen4Aegja4fBZ+LU=\",\"power\":\"1000000000000000000\"}],\"accounts\":[{\"meta\":{\"Account\":{\"owner\":\"f410fggjevhgketpz6gw6ordusynlgcd5piyug4aomuq\"}},\"balance\":\"1000000000000000000\"},{\"meta\":{\"Account\":{\"owner\":\"f410frbdnwklaitcjsqe7swjwp5naple6vthq4woyfry\"}},\"balance\":\"2000000000000000000\"},{\"meta\":{\"Account\":{\"owner\":\"f410fxo4lih4n2acr3oadalidwqjgoqkzhp5dw3zwkvy\"}},\"balance\":\"1000000000000000000\"}],\"ipc\":{\"gateway\":{\"subnet_id\":\"/r314159/f410fnfmitm2ww7oehhtbokf6wulhrr62sgq3sgqmenq\",\"bottom_up_check_period\":30,\"msg_fee\":\"1000000000000\",\"majority_percentage\":60,\"active_validators_limit\":100}}}";

        let genesis: Genesis = serde_json::from_str(genesis_json).expect("failed to parse genesis");

        let bundle = read_bundle();
        let custom_actors_bundle = read_custom_actors_bundle();
        let interpreter = make_interpreter();
        let multi_engine = Arc::new(MultiEngine::default());

        let store = MemoryBlockstore::new();
        let state =
            FvmGenesisState::new(store, multi_engine.clone(), &bundle, &custom_actors_bundle)
                .await
                .expect("failed to create state");

        let (state, _) = interpreter
            .init(state, genesis.clone())
            .await
            .expect("failed to create actors");

        let state_root_hash = state.commit().expect("failed to commit");

        let expected_root_hash =
            Cid::from_str("bafy2bzacedebgy4j7qnh2v2x4kkr2jqfkryql5ookbjrwge6dbrr24ytlqnj4")
                .unwrap();

        assert_eq!(state_root_hash, expected_root_hash);
    }

    fn make_genesis() -> Genesis {
        let mut g = quickcheck::Gen::new(5);
        let mut genesis = Genesis::arbitrary(&mut g);

        // Make sure we have IPC enabled.
        genesis.ipc = Some(IpcParams::arbitrary(&mut g));
        genesis
    }

    fn make_interpreter(
    ) -> FvmMessageInterpreter<MemoryBlockstore, MockClient<MockRequestMethodMatcher>> {
        let (client, _) = MockClient::new(MockRequestMethodMatcher::default());
        FvmMessageInterpreter::new(
            client,
            None,
            contracts_path(),
            1.05,
            1.05,
            false,
            UpgradeScheduler::new(),
        )
    }

    fn read_bundle() -> Vec<u8> {
        std::fs::read(bundle_path()).expect("failed to read bundle")
    }

    fn read_custom_actors_bundle() -> Vec<u8> {
        std::fs::read(custom_actors_bundle_path()).expect("failed to read custom actor bundle")
    }
}
