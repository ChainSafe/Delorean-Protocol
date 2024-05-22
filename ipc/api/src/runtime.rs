// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use crate::checkpoint::{BottomUpCheckpoint, Validators};
use fil_actors_runtime::runtime::Runtime;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;

impl BottomUpCheckpoint {
    /// Agents may set the source of a checkpoint using f2-based subnetIDs, \
    /// but actors are expected to use f0-based subnetIDs, thus the need to enforce
    /// that the source is a f0-based subnetID.
    pub fn enforce_f0_source(&mut self, rt: &impl Runtime) -> anyhow::Result<()> {
        self.data.source = self.source().f0_id(rt);
        Ok(())
    }
}

impl Validators {
    /// Get the weight of a validator
    /// It expects ID addresses as an input
    pub fn get_validator_weight(&self, rt: &impl Runtime, addr: &Address) -> Option<TokenAmount> {
        self.validators
            .validators()
            .iter()
            .find(|x| match rt.resolve_address(&x.addr) {
                Some(id) => id == *addr,
                None => false,
            })
            .map(|v| v.weight.clone())
    }
}
