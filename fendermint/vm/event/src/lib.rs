// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

pub type BlockHeight = u64;
/// Hex encoded block hash.
pub type BlockHashHex<'a> = &'a str;

#[derive(Debug, Default)]
pub struct NewParentView<'a> {
    pub is_null: bool,
    pub block_height: BlockHeight,
    pub block_hash: Option<BlockHashHex<'a>>, // hex encoded, unless null block
    pub num_msgs: usize,
    pub num_validator_changes: usize,
}

#[derive(Debug, Default)]
pub struct ParentFinalityCommitted<'a> {
    pub block_height: BlockHeight,
    pub block_hash: BlockHashHex<'a>,
}

#[derive(Debug, Default)]
pub struct NewBottomUpCheckpoint<'a> {
    pub block_height: BlockHeight,
    pub block_hash: BlockHashHex<'a>,
    pub num_msgs: usize,
    pub next_configuration_number: u64,
}

/// This node sees something as final, but it's missing the quorum for it.
///
/// The opposite does not happen because we only look for quorum for things we see as final.
#[derive(Debug, Default)]
pub struct ParentFinalityMissingQuorum<'a> {
    pub block_height: BlockHeight,
    pub block_hash: BlockHashHex<'a>,
}
