// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_actor_interface::ipc::subnet::SubnetActorErrors;
use fendermint_vm_genesis::{Collateral, Validator};
use fendermint_vm_interpreter::fvm::state::fevm::{
    ContractCaller, ContractResult, MockProvider, NoRevert,
};
use fendermint_vm_interpreter::fvm::state::FvmExecState;
use fendermint_vm_message::conv::{from_eth, from_fvm};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::crypto::signature::SECP_SIG_LEN;
use fvm_shared::econ::TokenAmount;
use ipc_actors_abis::subnet_actor_checkpointing_facet::{
    self as checkpointer, SubnetActorCheckpointingFacet,
};
use ipc_actors_abis::subnet_actor_getter_facet::{self as getter, SubnetActorGetterFacet};
use ipc_actors_abis::subnet_actor_manager_facet::SubnetActorManagerFacet;

pub use ipc_actors_abis::register_subnet_facet::ConstructorParams as SubnetConstructorParams;
use ipc_actors_abis::subnet_actor_reward_facet::SubnetActorRewardFacet;

#[derive(Clone)]
pub struct SubnetCaller<DB> {
    addr: EthAddress,
    getter: ContractCaller<DB, SubnetActorGetterFacet<MockProvider>, NoRevert>,
    manager: ContractCaller<DB, SubnetActorManagerFacet<MockProvider>, SubnetActorErrors>,
    rewarder: ContractCaller<DB, SubnetActorRewardFacet<MockProvider>, SubnetActorErrors>,
    checkpointer:
        ContractCaller<DB, SubnetActorCheckpointingFacet<MockProvider>, SubnetActorErrors>,
}

impl<DB> SubnetCaller<DB> {
    pub fn new(addr: EthAddress) -> Self {
        Self {
            addr,
            getter: ContractCaller::new(addr, SubnetActorGetterFacet::new),
            manager: ContractCaller::new(addr, SubnetActorManagerFacet::new),
            rewarder: ContractCaller::new(addr, SubnetActorRewardFacet::new),
            checkpointer: ContractCaller::new(addr, SubnetActorCheckpointingFacet::new),
        }
    }

    pub fn addr(&self) -> EthAddress {
        self.addr
    }
}

type TryCallResult<T> = anyhow::Result<ContractResult<T, SubnetActorErrors>>;

impl<DB: Blockstore + Clone> SubnetCaller<DB> {
    /// Join a subnet as a validator.
    pub fn join(
        &self,
        state: &mut FvmExecState<DB>,
        validator: &Validator<Collateral>,
    ) -> anyhow::Result<()> {
        let public_key = validator.public_key.0.serialize();
        let addr = EthAddress::new_secp256k1(&public_key)?;
        let deposit = from_fvm::to_eth_tokens(&validator.power.0)?;

        // We need to send in the name of the address as a sender, not the system account.
        self.manager.call(state, |c| {
            c.join(public_key.into()).from(addr).value(deposit)
        })
    }

    /// Try to join the subnet as a validator.
    pub fn try_join(
        &self,
        state: &mut FvmExecState<DB>,
        validator: &Validator<Collateral>,
    ) -> TryCallResult<()> {
        let public_key = validator.public_key.0.serialize();
        let addr = EthAddress::new_secp256k1(&public_key)?;
        let deposit = from_fvm::to_eth_tokens(&validator.power.0)?;
        self.manager.try_call(state, |c| {
            c.join(public_key.into()).from(addr).value(deposit)
        })
    }

    /// Try to increase the stake of a validator.
    pub fn try_stake(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
        value: &TokenAmount,
    ) -> TryCallResult<()> {
        let deposit = from_fvm::to_eth_tokens(value)?;
        self.manager
            .try_call(state, |c| c.stake().from(addr).value(deposit))
    }

    /// Try to decrease the stake of a validator.
    pub fn try_unstake(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
        value: &TokenAmount,
    ) -> TryCallResult<()> {
        let withdraw = from_fvm::to_eth_tokens(value)?;
        self.manager
            .try_call(state, |c| c.unstake(withdraw).from(addr))
    }

    /// Try to remove all stake of a validator.
    pub fn try_leave(&self, state: &mut FvmExecState<DB>, addr: &EthAddress) -> TryCallResult<()> {
        self.manager.try_call(state, |c| c.leave().from(addr))
    }

    /// Claim any refunds.
    pub fn try_claim(&self, state: &mut FvmExecState<DB>, addr: &EthAddress) -> TryCallResult<()> {
        self.rewarder.try_call(state, |c| c.claim().from(addr))
    }

    /// Submit a bottom-up checkpoint.
    pub fn try_submit_checkpoint(
        &self,
        state: &mut FvmExecState<DB>,
        checkpoint: checkpointer::BottomUpCheckpoint,
        _messages: Vec<checkpointer::IpcEnvelope>,
        signatures: Vec<(EthAddress, [u8; SECP_SIG_LEN])>,
    ) -> TryCallResult<()> {
        let mut addrs = Vec::new();
        let mut sigs = Vec::new();
        for (addr, sig) in signatures {
            addrs.push(ethers::types::Address::from(addr));
            sigs.push(sig.into());
        }
        self.checkpointer
            .try_call(state, |c| c.submit_checkpoint(checkpoint, addrs, sigs))
    }

    /// Get information about the validator's current and total collateral.
    pub fn get_validator(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
    ) -> anyhow::Result<getter::ValidatorInfo> {
        self.getter.call(state, |c| c.get_validator(addr.into()))
    }

    /// Get the confirmed collateral of a validator.
    pub fn confirmed_collateral(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
    ) -> anyhow::Result<TokenAmount> {
        self.get_validator(state, addr)
            .map(|i| from_eth::to_fvm_tokens(&i.confirmed_collateral))
    }

    /// Get the total (unconfirmed) collateral of a validator.
    pub fn total_collateral(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
    ) -> anyhow::Result<TokenAmount> {
        self.get_validator(state, addr)
            .map(|i| from_eth::to_fvm_tokens(&i.total_collateral))
    }

    /// Get the `(next, start)` configuration number pair.
    ///
    /// * `next` is the next expected one
    /// * `start` is the first unapplied one
    pub fn get_configuration_numbers(
        &self,
        state: &mut FvmExecState<DB>,
    ) -> anyhow::Result<(u64, u64)> {
        self.getter.call(state, |c| c.get_configuration_numbers())
    }

    /// Check if minimum collateral has been met.
    pub fn bootstrapped(&self, state: &mut FvmExecState<DB>) -> anyhow::Result<bool> {
        self.getter.call(state, |c| c.bootstrapped())
    }

    /// Check if a validator is active, ie. they are in the top N.
    pub fn is_active(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
    ) -> anyhow::Result<bool> {
        self.getter
            .call(state, |c| c.is_active_validator(addr.into()))
    }

    /// Check if a validator is wating, ie. they have deposited but are not in the top N.
    pub fn is_waiting(
        &self,
        state: &mut FvmExecState<DB>,
        addr: &EthAddress,
    ) -> anyhow::Result<bool> {
        self.getter
            .call(state, |c| c.is_waiting_validator(addr.into()))
    }

    /// This is purely for testing, although we could use it in production to avoid having to match Rust and Solidity semantics.
    pub fn cross_msgs_hash(
        &self,
        state: &mut FvmExecState<DB>,
        cross_msgs: Vec<getter::IpcEnvelope>,
    ) -> anyhow::Result<[u8; 32]> {
        self.getter.call(state, |c| c.cross_msgs_hash(cross_msgs))
    }
}
