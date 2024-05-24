// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use num_derive::FromPrimitive;

pub type BlockHeight = i64;
pub type Tag = [u8; 32];

pub const CETF_ACTOR_NAME: &str = "cetf";

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct EnqueueTagParams {
    pub tag: [u8; 32],
}

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct GetTagParams {
    pub height: BlockHeight,
}

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = frc42_dispatch::method_hash!("Constructor"),
    EnqueueTag = frc42_dispatch::method_hash!("EnqueueTag"),
    GetTag = frc42_dispatch::method_hash!("GetTag"),
}
