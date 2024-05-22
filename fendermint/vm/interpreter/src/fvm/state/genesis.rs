// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::{anyhow, bail, Context};
use cid::{multihash::Code, Cid};
use ethers::{abi::Tokenize, core::abi::Abi};
use fendermint_actors::Manifest as CustomActorManifest;
use fendermint_vm_actor_interface::{
    account::{self, ACCOUNT_ACTOR_CODE_ID},
    eam::{self, EthAddress},
    ethaccount::ETHACCOUNT_ACTOR_CODE_ID,
    evm,
    init::{self, builtin_actor_eth_addr},
    multisig::{self, MULTISIG_ACTOR_CODE_ID},
    system, EMPTY_ARR,
};
use fendermint_vm_core::Timestamp;
use fendermint_vm_genesis::{Account, Multisig, PowerScale};
use fvm::{
    engine::MultiEngine,
    machine::Manifest,
    state_tree::{ActorState, StateTree},
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::load_car_unchecked;
use fvm_ipld_encoding::{BytesDe, CborStore, RawBytes};
use fvm_shared::{
    address::{Address, Payload},
    clock::ChainEpoch,
    econ::TokenAmount,
    message::Message,
    state::StateTreeVersion,
    version::NetworkVersion,
    ActorID, BLOCK_GAS_LIMIT, METHOD_CONSTRUCTOR,
};
use num_traits::Zero;
use serde::{de, Serialize};

use super::{exec::MachineBlockstore, FvmExecState, FvmStateParams};

/// Create an empty state tree.
pub fn empty_state_tree<DB: Blockstore>(store: DB) -> anyhow::Result<StateTree<DB>> {
    let state_tree = StateTree::new(store, StateTreeVersion::V5)?;
    Ok(state_tree)
}

/// Initially we can only set up an empty state tree.
/// Then we have to create the built-in actors' state that the FVM relies on.
/// Then we can instantiate an FVM execution engine, which we can use to construct FEVM based actors.
enum Stage<DB: Blockstore + Clone + 'static> {
    Tree(StateTree<DB>),
    Exec(FvmExecState<DB>),
}

/// A state we create for the execution of genesis initialisation.
pub struct FvmGenesisState<DB>
where
    DB: Blockstore + Clone + 'static,
{
    pub manifest_data_cid: Cid,
    pub manifest: Manifest,
    pub custom_actor_manifest: CustomActorManifest,
    store: DB,
    multi_engine: Arc<MultiEngine>,
    stage: Stage<DB>,
}

async fn parse_bundle<DB: Blockstore>(store: &DB, bundle: &[u8]) -> anyhow::Result<(u32, Cid)> {
    let bundle_roots = load_car_unchecked(&store, bundle).await?;
    let bundle_root = match bundle_roots.as_slice() {
        [root] => root,
        roots => {
            return Err(anyhow!(
                "expected one root in builtin actor bundle; got {}",
                roots.len()
            ))
        }
    };

    let (manifest_version, manifest_data_cid): (u32, Cid) = match store.get_cbor(bundle_root)? {
        Some(vd) => vd,
        None => {
            return Err(anyhow!(
                "no manifest information in bundle root {}",
                bundle_root
            ))
        }
    };

    Ok((manifest_version, manifest_data_cid))
}

impl<DB> FvmGenesisState<DB>
where
    DB: Blockstore + Clone + 'static,
{
    pub async fn new(
        store: DB,
        multi_engine: Arc<MultiEngine>,
        bundle: &[u8],
        custom_actor_bundle: &[u8],
    ) -> anyhow::Result<Self> {
        // Load the builtin actor bundle.
        let (manifest_version, manifest_data_cid): (u32, Cid) =
            parse_bundle(&store, bundle).await?;
        let manifest = Manifest::load(&store, &manifest_data_cid, manifest_version)?;

        // Load the custom actor bundle.
        let (custom_manifest_version, custom_manifest_data_cid): (u32, Cid) =
            parse_bundle(&store, custom_actor_bundle).await?;
        let custom_actor_manifest =
            CustomActorManifest::load(&store, &custom_manifest_data_cid, custom_manifest_version)?;

        let state_tree = empty_state_tree(store.clone())?;

        let state = Self {
            manifest_data_cid,
            manifest,
            custom_actor_manifest,
            store,
            multi_engine,
            stage: Stage::Tree(state_tree),
        };

        Ok(state)
    }

    /// Instantiate the execution state, once the basic genesis parameters are known.
    ///
    /// This must be called before we try to instantiate any EVM actors in genesis.
    pub fn init_exec_state(
        &mut self,
        timestamp: Timestamp,
        network_version: NetworkVersion,
        base_fee: TokenAmount,
        circ_supply: TokenAmount,
        chain_id: u64,
        power_scale: PowerScale,
    ) -> anyhow::Result<()> {
        self.stage = match self.stage {
            Stage::Exec(_) => bail!("execution engine already initialized"),
            Stage::Tree(ref mut state_tree) => {
                // We have to flush the data at this point.
                let state_root = state_tree.flush()?;

                let params = FvmStateParams {
                    state_root,
                    timestamp,
                    network_version,
                    base_fee,
                    circ_supply,
                    chain_id,
                    power_scale,
                    app_version: 0,
                };

                let exec_state =
                    FvmExecState::new(self.store.clone(), &self.multi_engine, 1, params)
                        .context("failed to create exec state")?;

                Stage::Exec(exec_state)
            }
        };
        Ok(())
    }

    /// Flush the data to the block store.
    pub fn commit(self) -> anyhow::Result<Cid> {
        match self.stage {
            Stage::Tree(mut state_tree) => Ok(state_tree.flush()?),
            Stage::Exec(exec_state) => match exec_state.commit()? {
                (_, _, true) => bail!("FVM parameters are not expected to be updated in genesis"),
                (cid, _, _) => Ok(cid),
            },
        }
    }

    /// Replaces the built in actor with custom actor. This assumes the system actor is already
    /// created, else it would throw an error.
    pub fn replace_builtin_actor(
        &mut self,
        built_in_actor_name: &str,
        built_in_actor_id: ActorID,
        custom_actor_name: &str,
        state: &impl Serialize,
        balance: TokenAmount,
        delegated_address: Option<Address>,
    ) -> anyhow::Result<()> {
        let code_cid = self
            .update_system_actor_manifest(built_in_actor_name, custom_actor_name)
            .context("failed to replace system actor manifest")?;

        self.create_actor_internal(
            code_cid,
            built_in_actor_id,
            state,
            balance,
            delegated_address,
        )
    }

    /// Update the manifest id of the system actor, returns the code cid of the replacing
    /// custom actor.
    fn update_system_actor_manifest(
        &mut self,
        built_in_actor_name: &str,
        custom_actor_name: &str,
    ) -> anyhow::Result<Cid> {
        let code = *self
            .custom_actor_manifest
            .code_by_name(custom_actor_name)
            .ok_or_else(|| anyhow!("replacement {custom_actor_name} actor not found"))?;

        let manifest_cid = self
            .get_actor_state::<system::State>(system::SYSTEM_ACTOR_ID)?
            .builtin_actors;

        let mut built_in_actors: Vec<(String, Cid)> = self
            .store()
            .get_cbor(&manifest_cid)
            .context("could not load built in actors")?
            .ok_or_else(|| anyhow!("cannot find manifest cid {}", manifest_cid))?;

        for (_, code_cid) in built_in_actors
            .iter_mut()
            .filter(|(n, _)| n == built_in_actor_name)
        {
            *code_cid = code
        }

        let builtin_actors = self.put_state(built_in_actors)?;
        let new_cid = self.put_state(system::State { builtin_actors })?;
        let mutate = |actor_state: &mut ActorState| {
            actor_state.state = new_cid;
            Ok(())
        };

        self.with_state_tree(
            |s| s.mutate_actor(system::SYSTEM_ACTOR_ID, mutate),
            |s| s.mutate_actor(system::SYSTEM_ACTOR_ID, mutate),
        )?;

        Ok(code)
    }

    pub fn create_builtin_actor(
        &mut self,
        code_id: u32,
        id: ActorID,
        state: &impl Serialize,
        balance: TokenAmount,
        delegated_address: Option<Address>,
    ) -> anyhow::Result<()> {
        // Retrieve the CID of the actor code by the numeric ID.
        let code_cid = *self
            .manifest
            .code_by_id(code_id)
            .ok_or_else(|| anyhow!("can't find {code_id} in the manifest"))?;

        self.create_actor_internal(code_cid, id, state, balance, delegated_address)
    }

    pub fn create_custom_actor(
        &mut self,
        name: &str,
        id: ActorID,
        state: &impl Serialize,
        balance: TokenAmount,
        delegated_address: Option<Address>,
    ) -> anyhow::Result<()> {
        // Retrieve the CID of the actor code by the numeric ID.
        let code_cid = *self
            .custom_actor_manifest
            .code_by_name(name)
            .ok_or_else(|| anyhow!("can't find actor: {name} in the custom actor manifest"))?;

        self.create_actor_internal(code_cid, id, state, balance, delegated_address)
    }

    /// Creates an actor using code specified in the manifest.
    fn create_actor_internal(
        &mut self,
        code_cid: Cid,
        id: ActorID,
        state: &impl Serialize,
        balance: TokenAmount,
        delegated_address: Option<Address>,
    ) -> anyhow::Result<()> {
        let state_cid = self.put_state(state)?;

        let actor_state = ActorState {
            code: code_cid,
            state: state_cid,
            sequence: 0,
            balance,
            delegated_address,
        };

        self.with_state_tree(
            |s| s.set_actor(id, actor_state.clone()),
            |s| s.set_actor(id, actor_state.clone()),
        );

        {
            let cid = self.with_state_tree(|s| s.flush(), |s| s.flush())?;
            tracing::debug!(
                state_root = cid.to_string(),
                actor_id = id,
                "interim state root after actor creation"
            );
        }

        Ok(())
    }

    pub fn create_account_actor(
        &mut self,
        acct: Account,
        balance: TokenAmount,
        ids: &init::AddressMap,
    ) -> anyhow::Result<()> {
        let owner = acct.owner.0;

        let id = ids
            .get(&owner)
            .ok_or_else(|| anyhow!("can't find ID for owner {owner}"))?;

        match owner.payload() {
            Payload::Secp256k1(_) => {
                let state = account::State { address: owner };
                self.create_builtin_actor(ACCOUNT_ACTOR_CODE_ID, *id, &state, balance, None)
            }
            Payload::Delegated(d) if d.namespace() == eam::EAM_ACTOR_ID => {
                let state = EMPTY_ARR;
                // NOTE: Here we could use the placeholder code ID as well.
                self.create_builtin_actor(
                    ETHACCOUNT_ACTOR_CODE_ID,
                    *id,
                    &state,
                    balance,
                    Some(owner),
                )
            }
            other => Err(anyhow!("unexpected actor owner: {other:?}")),
        }
    }

    pub fn create_multisig_actor(
        &mut self,
        ms: Multisig,
        balance: TokenAmount,
        ids: &init::AddressMap,
        next_id: ActorID,
    ) -> anyhow::Result<()> {
        let mut signers = Vec::new();

        // Make sure every signer has their own account.
        for signer in ms.signers {
            let id = ids
                .get(&signer.0)
                .ok_or_else(|| anyhow!("can't find ID for signer {}", signer.0))?;

            if self
                .with_state_tree(|s| s.get_actor(*id), |s| s.get_actor(*id))?
                .is_none()
            {
                self.create_account_actor(Account { owner: signer }, TokenAmount::zero(), ids)?;
            }

            signers.push(*id)
        }

        // Now create a multisig actor that manages group transactions.
        let state = multisig::State::new(
            self.store(),
            signers,
            ms.threshold,
            ms.vesting_start as ChainEpoch,
            ms.vesting_duration as ChainEpoch,
            balance.clone(),
        )?;

        self.create_builtin_actor(MULTISIG_ACTOR_CODE_ID, next_id, &state, balance, None)
    }

    /// Deploy an EVM contract with a fixed ID and some constructor arguments.
    ///
    /// Returns the hashed Ethereum address we can use to invoke the contract.
    pub fn create_evm_actor_with_cons<T: Tokenize>(
        &mut self,
        id: ActorID,
        abi: &Abi,
        bytecode: Vec<u8>,
        constructor_params: T,
    ) -> anyhow::Result<EthAddress> {
        let constructor = abi
            .constructor()
            .ok_or_else(|| anyhow!("contract doesn't have a constructor"))?;
        let initcode = constructor
            .encode_input(bytecode, &constructor_params.into_tokens())
            .context("failed to encode constructor input")?;

        self.create_evm_actor(id, initcode)
    }

    /// Deploy an EVM contract.
    ///
    /// Returns the hashed Ethereum address we can use to invoke the contract.
    pub fn create_evm_actor(
        &mut self,
        id: ActorID,
        initcode: Vec<u8>,
    ) -> anyhow::Result<EthAddress> {
        // Here we are circumventing the normal way of creating an actor through the EAM and jump ahead to what the `Init` actor would do:
        // https://github.com/filecoin-project/builtin-actors/blob/421855a7b968114ac59422c1faeca968482eccf4/actors/init/src/lib.rs#L97-L107

        // Based on how the EAM constructs it.
        let params = evm::ConstructorParams {
            // We have to pick someone as creator for these quasi built-in types.
            creator: EthAddress::from_id(system::SYSTEM_ACTOR_ID),
            initcode: RawBytes::from(initcode),
        };
        let params = RawBytes::serialize(params)?;

        // When a contract is constructed the EVM actor verifies that it has an Ethereum delegated address.
        // This has been inserted into the Init actor state as well.
        let f0_addr = Address::new_id(id);
        let f4_addr = Address::from(builtin_actor_eth_addr(id));

        let msg = Message {
            version: 0,
            from: init::INIT_ACTOR_ADDR, // asserted by the constructor
            to: f0_addr,
            sequence: 0, // We will use implicit execution which doesn't check or modify this.
            value: TokenAmount::zero(),
            method_num: METHOD_CONSTRUCTOR,
            params,
            gas_limit: BLOCK_GAS_LIMIT,
            gas_fee_cap: TokenAmount::zero(),
            gas_premium: TokenAmount::zero(),
        };

        // Create an empty actor to receive the call.
        self.create_builtin_actor(
            evm::EVM_ACTOR_CODE_ID,
            id,
            &EMPTY_ARR,
            TokenAmount::zero(),
            Some(f4_addr),
        )
        .context("failed to create empty actor")?;

        let (apply_ret, _) = match self.stage {
            Stage::Tree(_) => bail!("execution engine not initialized"),
            Stage::Exec(ref mut exec_state) => exec_state
                .execute_implicit(msg)
                .context("failed to execute message")?,
        };

        {
            let cid = self.with_state_tree(|s| s.flush(), |s| s.flush())?;
            tracing::debug!(
                state_root = cid.to_string(),
                actor_id = id,
                "interim state root after EVM actor initialisation"
            );
        }

        if !apply_ret.msg_receipt.exit_code.is_success() {
            let error_data = apply_ret.msg_receipt.return_data;
            let error_data = if error_data.is_empty() {
                Vec::new()
            } else {
                // The EVM actor might return some revert in the output.
                error_data
                    .deserialize::<BytesDe>()
                    .map(|bz| bz.0)
                    .context("failed to deserialize error data")?
            };

            bail!(
                "failed to deploy EVM actor: code = {}; data = 0x{}; info = {:?}",
                apply_ret.msg_receipt.exit_code,
                hex::encode(error_data),
                apply_ret.failure_info,
            );
        }

        let addr: [u8; 20] = match f4_addr.payload() {
            Payload::Delegated(addr) => addr.subaddress().try_into().expect("hash is 20 bytes"),
            other => panic!("not an f4 address: {other:?}"),
        };

        Ok(EthAddress(addr))
    }

    pub fn store(&self) -> &DB {
        &self.store
    }

    pub fn exec_state(&mut self) -> Option<&mut FvmExecState<DB>> {
        match self.stage {
            Stage::Tree(_) => None,
            Stage::Exec(ref mut exec) => Some(exec),
        }
    }

    pub fn into_exec_state(self) -> Result<FvmExecState<DB>, Self> {
        match self.stage {
            Stage::Tree(_) => Err(self),
            Stage::Exec(exec) => Ok(exec),
        }
    }

    fn put_state(&mut self, state: impl Serialize) -> anyhow::Result<Cid> {
        self.store()
            .put_cbor(&state, Code::Blake2b256)
            .context("failed to store actor state")
    }

    /// A horrible way of unifying the state tree under the two different stages.
    ///
    /// We only use this a few times, so perhaps it's not that much of a burden to duplicate some code.
    fn with_state_tree<F, G, T>(&mut self, f: F, g: G) -> T
    where
        F: FnOnce(&mut StateTree<DB>) -> T,
        G: FnOnce(&mut StateTree<MachineBlockstore<DB>>) -> T,
    {
        match self.stage {
            Stage::Tree(ref mut state_tree) => f(state_tree),
            Stage::Exec(ref mut exec_state) => g(exec_state.state_tree_mut()),
        }
    }

    /// Query the actor state from the state tree under the two different stages.
    fn get_actor_state<T: de::DeserializeOwned>(&self, actor: ActorID) -> anyhow::Result<T> {
        let actor_state_cid = match &self.stage {
            Stage::Tree(s) => s.get_actor(actor)?,
            Stage::Exec(s) => s.state_tree().get_actor(actor)?,
        }
        .ok_or_else(|| anyhow!("actor state {actor} not found, is it deployed?"))?
        .state;

        self.store()
            .get_cbor(&actor_state_cid)
            .context("failed to get actor state by state cid")?
            .ok_or_else(|| anyhow!("actor state by {actor_state_cid} not found"))
    }
}
