// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

/// This type definitions are borrowed from
/// https://github.com/consensus-shipyard/ipc-actors/tree/main/gateway
/// to ensure that they are in sync in this project.
/// However, we should either deprecate the native actors, or make
/// them use the types from this sdk directly.
///
use crate::subnet_id::SubnetID;
use cid::Cid;
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use fvm_shared::address::Address;

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct FundParams {
    pub subnet: SubnetID,
    pub to: Address,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct ReleaseParams {
    pub to: Address,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct PropagateParams {
    /// The postbox message cid
    pub postbox_cid: Cid,
}
