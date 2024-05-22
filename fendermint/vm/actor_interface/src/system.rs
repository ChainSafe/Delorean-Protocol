// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::address::Address;
use lazy_static::lazy_static;

use crate::eam::EthAddress;

define_singleton!(SYSTEM { id: 0, code_id: 1 });

lazy_static! {
    /// The Ethereum null-address 0x00..00 can also be used to identify the system actor.
    pub static ref SYSTEM_ACTOR_ETH_ADDR: Address = EthAddress::default().into();
}

/// Check whether the address is one of those identifying the system actor.
pub fn is_system_addr(addr: &Address) -> bool {
    *addr == SYSTEM_ACTOR_ADDR || *addr == *SYSTEM_ACTOR_ETH_ADDR
}

/// System actor state.
#[derive(Default, Deserialize_tuple, Serialize_tuple, Debug, Clone)]
pub struct State {
    // builtin actor registry: Vec<(String, Cid)>
    pub builtin_actors: Cid,
}
