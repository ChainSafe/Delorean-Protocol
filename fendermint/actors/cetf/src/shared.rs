use fil_actors_runtime::MapKey;
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_ipld_encoding::{
    strict_bytes,
    tuple::{Deserialize_tuple, Serialize_tuple},
};
use fvm_shared::address::Address;
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};

pub type BlockHeight = u64;
pub type Tag = [u8; 32];
/// A BLS Public Key used for signing tags.
#[derive(Deserialize, Serialize, Clone, Copy, Eq, PartialEq, Debug)]
#[serde(transparent)]
pub struct BlsPublicKey(#[serde(with = "strict_bytes")] pub [u8; 48]);
impl Default for BlsPublicKey {
    fn default() -> Self {
        BlsPublicKey([0; 48])
    }
}
impl From<[u8; 48]> for BlsPublicKey {
    fn from(bytes: [u8; 48]) -> Self {
        BlsPublicKey(bytes)
    }
}
impl From<&[u8; 48]> for BlsPublicKey {
    fn from(bytes: &[u8; 48]) -> Self {
        BlsPublicKey(*bytes)
    }
}

/// A BLS Public Key used for signing tags.
#[derive(Deserialize, Serialize, Clone, Copy, Eq, PartialEq, Debug)]
#[serde(transparent)]
pub struct BlsSignature(#[serde(with = "strict_bytes")] pub [u8; 96]);
impl Default for BlsSignature {
    fn default() -> Self {
        BlsSignature([0; 96])
    }
}
impl From<[u8; 96]> for BlsSignature {
    fn from(bytes: [u8; 96]) -> Self {
        BlsSignature(bytes)
    }
}
impl From<&[u8; 96]> for BlsSignature {
    fn from(bytes: &[u8; 96]) -> Self {
        BlsSignature(*bytes)
    }
}

pub const CETF_ACTOR_NAME: &str = "cetf";

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct EnqueueTagParams {
    pub tag: [u8; 32],
}

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct GetTagParams {
    pub height: BlockHeight,
}
#[derive(Debug, Serialize_tuple, Deserialize_tuple)]

pub struct AddValidatorParams {
    pub address: Address,
    pub public_key: BlsPublicKey,
}

#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct AddSignedTagParams {
    pub height: BlockHeight,
    pub signature: BlsSignature,
}

#[derive(Deserialize, Serialize, Clone, Copy, Eq, PartialEq, Debug, Default)]
#[serde(transparent)]
pub struct BlockHash(#[serde(with = "strict_bytes")] pub [u8; 32]);

impl MapKey for BlockHash {
    fn to_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.0.to_vec())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut buf = [0; 32];
        buf.copy_from_slice(&bytes);
        Ok(BlockHash(buf))
    }
}

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct AddSignedBlockHeightTagParams {
    pub height: BlockHeight,
    pub signature: BlsSignature,
}

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct AddSignedBlockHashTagParams {
    pub hash: BlockHash,
    pub signature: BlsSignature,
}

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = frc42_dispatch::method_hash!("Constructor"),
    EnqueueTag = frc42_dispatch::method_hash!("EnqueueTag"),
    GetTag = frc42_dispatch::method_hash!("GetTag"),
    AddValidator = frc42_dispatch::method_hash!("AddValidator"),
    Enable = frc42_dispatch::method_hash!("Enable"),
    Disable = frc42_dispatch::method_hash!("Disable"),
    AddSignedTag = frc42_dispatch::method_hash!("AddSignedTag"),
    AddSignedBlockHashTag = frc42_dispatch::method_hash!("AddSignedBlockHashTag"),
    AddSignedBlockHeightTag = frc42_dispatch::method_hash!("AddSignedBlockHeightTag"),
}
