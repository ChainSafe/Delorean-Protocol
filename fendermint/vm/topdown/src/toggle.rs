// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::finality::ParentViewPayload;
use crate::{
    BlockHash, BlockHeight, CachedFinalityProvider, Error, IPCParentFinality,
    ParentFinalityProvider, ParentViewProvider,
};
use anyhow::anyhow;
use async_stm::{Stm, StmResult};
use ipc_api::cross::IpcEnvelope;
use ipc_api::staking::StakingChangeRequest;

/// The parent finality provider could have all functionalities disabled.
#[derive(Clone)]
pub struct Toggle<P> {
    inner: Option<P>,
}

impl<P> Toggle<P> {
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn enabled(inner: P) -> Self {
        Self { inner: Some(inner) }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    fn perform_or_else<F, T, E>(&self, f: F, other: T) -> Result<T, E>
    where
        F: FnOnce(&P) -> Result<T, E>,
    {
        match &self.inner {
            Some(p) => f(p),
            None => Ok(other),
        }
    }
}

#[async_trait::async_trait]
impl<P: ParentViewProvider + Send + Sync + 'static> ParentViewProvider for Toggle<P> {
    fn genesis_epoch(&self) -> anyhow::Result<BlockHeight> {
        match self.inner.as_ref() {
            Some(p) => p.genesis_epoch(),
            None => Err(anyhow!("provider is toggled off")),
        }
    }

    async fn validator_changes_from(
        &self,
        from: BlockHeight,
        to: BlockHeight,
    ) -> anyhow::Result<Vec<StakingChangeRequest>> {
        match self.inner.as_ref() {
            Some(p) => p.validator_changes_from(from, to).await,
            None => Err(anyhow!("provider is toggled off")),
        }
    }

    async fn top_down_msgs_from(
        &self,
        from: BlockHeight,
        to: BlockHeight,
    ) -> anyhow::Result<Vec<IpcEnvelope>> {
        match self.inner.as_ref() {
            Some(p) => p.top_down_msgs_from(from, to).await,
            None => Err(anyhow!("provider is toggled off")),
        }
    }
}

impl<P: ParentFinalityProvider + Send + Sync + 'static> ParentFinalityProvider for Toggle<P> {
    fn next_proposal(&self) -> Stm<Option<IPCParentFinality>> {
        self.perform_or_else(|p| p.next_proposal(), None)
    }

    fn check_proposal(&self, proposal: &IPCParentFinality) -> Stm<bool> {
        self.perform_or_else(|p| p.check_proposal(proposal), false)
    }

    fn set_new_finality(
        &self,
        finality: IPCParentFinality,
        previous_finality: Option<IPCParentFinality>,
    ) -> Stm<()> {
        self.perform_or_else(|p| p.set_new_finality(finality, previous_finality), ())
    }
}

impl<P> Toggle<CachedFinalityProvider<P>> {
    pub fn block_hash(&self, height: BlockHeight) -> Stm<Option<BlockHash>> {
        self.perform_or_else(|p| p.block_hash(height), None)
    }

    pub fn latest_height_in_cache(&self) -> Stm<Option<BlockHeight>> {
        self.perform_or_else(|p| p.latest_height_in_cache(), None)
    }

    pub fn latest_height(&self) -> Stm<Option<BlockHeight>> {
        self.perform_or_else(|p| p.latest_height(), None)
    }

    pub fn last_committed_finality(&self) -> Stm<Option<IPCParentFinality>> {
        self.perform_or_else(|p| p.last_committed_finality(), None)
    }

    pub fn new_parent_view(
        &self,
        height: BlockHeight,
        maybe_payload: Option<ParentViewPayload>,
    ) -> StmResult<(), Error> {
        self.perform_or_else(|p| p.new_parent_view(height, maybe_payload), ())
    }

    pub fn reset(&self, finality: IPCParentFinality) -> Stm<()> {
        self.perform_or_else(|p| p.reset(finality), ())
    }

    pub fn cached_blocks(&self) -> Stm<BlockHeight> {
        self.perform_or_else(|p| p.cached_blocks(), BlockHeight::MAX)
    }

    pub fn first_non_null_block(&self, height: BlockHeight) -> Stm<Option<BlockHeight>> {
        self.perform_or_else(|p| p.first_non_null_block(height), None)
    }
}
