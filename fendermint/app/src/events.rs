// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BlockHeight;

/// Re-export other events, just to provide the visibility of where they are.
pub use fendermint_vm_event::{
    NewBottomUpCheckpoint, NewParentView, ParentFinalityCommitted, ParentFinalityMissingQuorum,
};

/// Hex encoded block hash.
pub type BlockHashHex<'a> = &'a str;

#[derive(Debug, Default)]
pub struct ProposalProcessed<'a> {
    pub is_accepted: bool,
    pub block_height: BlockHeight,
    pub block_hash: BlockHashHex<'a>,
    pub num_txs: usize,
    pub proposer: &'a str,
}

#[derive(Debug, Default)]
pub struct NewBlock {
    pub block_height: BlockHeight,
}

#[derive(Debug, Default)]
pub struct ParentFinalityVoteAdded<'a> {
    pub block_height: BlockHeight,
    pub block_hash: BlockHashHex<'a>,
    pub validator: &'a str,
}

#[derive(Debug, Default)]
pub struct ParentFinalityVoteIgnored<'a> {
    pub block_height: BlockHeight,
    pub block_hash: BlockHashHex<'a>,
    pub validator: &'a str,
}

// TODO: Add new events for:
// * snapshots
