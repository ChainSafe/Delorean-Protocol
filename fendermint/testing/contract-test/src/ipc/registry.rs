// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_actor_interface::init::builtin_actor_eth_addr;
use fendermint_vm_actor_interface::ipc::SUBNETREGISTRY_ACTOR_ID;
use fendermint_vm_interpreter::fvm::state::fevm::{ContractCaller, MockProvider};
use fendermint_vm_interpreter::fvm::state::FvmExecState;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::ActorID;
use ipc_actors_abis::subnet_registry_diamond::SubnetRegistryDiamondErrors;

pub use ipc_actors_abis::register_subnet_facet::{
    ConstructorParams as SubnetConstructorParams, RegisterSubnetFacet,
};

#[derive(Clone)]
pub struct RegistryCaller<DB> {
    addr: EthAddress,
    register: ContractCaller<DB, RegisterSubnetFacet<MockProvider>, SubnetRegistryDiamondErrors>,
}

impl<DB> Default for RegistryCaller<DB> {
    fn default() -> Self {
        Self::new(SUBNETREGISTRY_ACTOR_ID)
    }
}

impl<DB> RegistryCaller<DB> {
    pub fn new(actor_id: ActorID) -> Self {
        let addr = builtin_actor_eth_addr(actor_id);
        Self {
            addr,
            register: ContractCaller::new(addr, RegisterSubnetFacet::new),
        }
    }

    pub fn addr(&self) -> EthAddress {
        self.addr
    }
}

impl<DB: Blockstore + Clone> RegistryCaller<DB> {
    /// Create a new instance of the built-in subnet implemetation.
    ///
    /// Returns the address of the deployed contract.
    pub fn new_subnet(
        &self,
        state: &mut FvmExecState<DB>,
        params: SubnetConstructorParams,
    ) -> anyhow::Result<EthAddress> {
        let addr = self
            .register
            .call(state, |c| c.new_subnet_actor(params))
            .context("failed to create new subnet")?;
        Ok(EthAddress(addr.0))
    }
}
