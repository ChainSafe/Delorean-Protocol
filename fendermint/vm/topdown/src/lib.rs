// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

mod cache;
mod error;
mod finality;
pub mod sync;

pub mod convert;
pub mod proxy;
mod toggle;
pub mod voting;

use async_stm::Stm;
use async_trait::async_trait;
use ethers::utils::hex;
use fvm_shared::clock::ChainEpoch;
use ipc_api::cross::IpcEnvelope;
use ipc_api::staking::StakingChangeRequest;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::time::Duration;

pub use crate::cache::{SequentialAppendError, SequentialKeyCache, ValueIter};
pub use crate::error::Error;
pub use crate::finality::CachedFinalityProvider;
pub use crate::toggle::Toggle;

pub type BlockHeight = u64;
pub type Bytes = Vec<u8>;
pub type BlockHash = Bytes;

/// The null round error message
pub(crate) const NULL_ROUND_ERR_MSG: &str = "requested epoch was a null round";
/// Default topdown proposal height range
pub(crate) const DEFAULT_MAX_PROPOSAL_RANGE: BlockHeight = 100;
pub(crate) const DEFAULT_MAX_CACHE_BLOCK: BlockHeight = 500;
pub(crate) const DEFAULT_PROPOSAL_DELAY: BlockHeight = 2;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// The number of blocks to delay before reporting a height as final on the parent chain.
    /// To propose a certain number of epochs delayed from the latest height, we see to be
    /// conservative and avoid other from rejecting the proposal because they don't see the
    /// height as final yet.
    pub chain_head_delay: BlockHeight,
    /// Parent syncing cron period, in seconds
    pub polling_interval: Duration,
    /// Top down exponential back off retry base
    pub exponential_back_off: Duration,
    /// The max number of retries for exponential backoff before giving up
    pub exponential_retry_limit: usize,
    /// The max number of blocks one should make the topdown proposal
    pub max_proposal_range: Option<BlockHeight>,
    /// Max number of blocks that should be stored in cache
    pub max_cache_blocks: Option<BlockHeight>,
    pub proposal_delay: Option<BlockHeight>,
}

impl Config {
    pub fn new(
        chain_head_delay: BlockHeight,
        polling_interval: Duration,
        exponential_back_off: Duration,
        exponential_retry_limit: usize,
    ) -> Self {
        Self {
            chain_head_delay,
            polling_interval,
            exponential_back_off,
            exponential_retry_limit,
            max_proposal_range: None,
            max_cache_blocks: None,
            proposal_delay: None,
        }
    }

    pub fn with_max_proposal_range(mut self, max_proposal_range: BlockHeight) -> Self {
        self.max_proposal_range = Some(max_proposal_range);
        self
    }

    pub fn with_proposal_delay(mut self, proposal_delay: BlockHeight) -> Self {
        self.proposal_delay = Some(proposal_delay);
        self
    }

    pub fn with_max_cache_blocks(mut self, max_cache_blocks: BlockHeight) -> Self {
        self.max_cache_blocks = Some(max_cache_blocks);
        self
    }

    pub fn max_proposal_range(&self) -> BlockHeight {
        self.max_proposal_range
            .unwrap_or(DEFAULT_MAX_PROPOSAL_RANGE)
    }

    pub fn proposal_delay(&self) -> BlockHeight {
        self.proposal_delay.unwrap_or(DEFAULT_PROPOSAL_DELAY)
    }

    pub fn max_cache_blocks(&self) -> BlockHeight {
        self.max_cache_blocks.unwrap_or(DEFAULT_MAX_CACHE_BLOCK)
    }
}

/// The finality view for IPC parent at certain height.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IPCParentFinality {
    /// The latest chain height
    pub height: BlockHeight,
    /// The block hash. For FVM, it is a Cid. For Evm, it is bytes32 as one can now potentially
    /// deploy a subnet on EVM.
    pub block_hash: BlockHash,
}

impl IPCParentFinality {
    pub fn new(height: ChainEpoch, hash: BlockHash) -> Self {
        Self {
            height: height as BlockHeight,
            block_hash: hash,
        }
    }
}

impl Display for IPCParentFinality {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "IPCParentFinality(height: {}, block_hash: {})",
            self.height,
            hex::encode(&self.block_hash)
        )
    }
}

#[async_trait]
pub trait ParentViewProvider {
    /// Obtain the genesis epoch of the current subnet in the parent
    fn genesis_epoch(&self) -> anyhow::Result<BlockHeight>;
    /// Get the validator changes from and to height.
    async fn validator_changes_from(
        &self,
        from: BlockHeight,
        to: BlockHeight,
    ) -> anyhow::Result<Vec<StakingChangeRequest>>;
    /// Get the top down messages from and to height.
    async fn top_down_msgs_from(
        &self,
        from: BlockHeight,
        to: BlockHeight,
    ) -> anyhow::Result<Vec<IpcEnvelope>>;
}

pub trait ParentFinalityProvider: ParentViewProvider {
    /// Latest proposal for parent finality
    fn next_proposal(&self) -> Stm<Option<IPCParentFinality>>;
    /// Check if the target proposal is valid
    fn check_proposal(&self, proposal: &IPCParentFinality) -> Stm<bool>;
    /// Called when finality is committed
    fn set_new_finality(
        &self,
        finality: IPCParentFinality,
        previous_finality: Option<IPCParentFinality>,
    ) -> Stm<()>;
}

/// If res is null round error, returns the default value from f()
pub(crate) fn handle_null_round<T, F: FnOnce() -> T>(
    res: anyhow::Result<T>,
    f: F,
) -> anyhow::Result<T> {
    match res {
        Ok(t) => Ok(t),
        Err(e) => {
            if is_null_round_error(&e) {
                Ok(f())
            } else {
                Err(e)
            }
        }
    }
}

pub(crate) fn is_null_round_error(err: &anyhow::Error) -> bool {
    is_null_round_str(&err.to_string())
}

pub(crate) fn is_null_round_str(s: &str) -> bool {
    s.contains(NULL_ROUND_ERR_MSG)
}
