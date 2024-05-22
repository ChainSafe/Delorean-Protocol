// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! The tendermint aware syncer

use crate::proxy::ParentQueryProxy;
use crate::sync::syncer::LotusParentSyncer;
use crate::sync::ParentFinalityStateQuery;
use anyhow::Context;

/// Tendermint aware syncer
pub(crate) struct TendermintAwareSyncer<T, C, P> {
    inner: LotusParentSyncer<T, P>,
    tendermint_client: C,
}

impl<T, C, P> TendermintAwareSyncer<T, C, P>
where
    T: ParentFinalityStateQuery + Send + Sync + 'static,
    C: tendermint_rpc::Client + Send + Sync + 'static,
    P: ParentQueryProxy + Send + Sync + 'static,
{
    pub fn new(inner: LotusParentSyncer<T, P>, tendermint_client: C) -> Self {
        Self {
            inner,
            tendermint_client,
        }
    }

    /// Sync with the parent, unless CometBFT is still catching up with the network,
    /// in which case we'll get the changes from the subnet peers in the blocks.
    pub async fn sync(&mut self) -> anyhow::Result<()> {
        if self.is_syncing_peer().await? {
            tracing::debug!("syncing with peer, skip parent finality syncing this round");
            return Ok(());
        }
        self.inner.sync().await
    }

    async fn is_syncing_peer(&self) -> anyhow::Result<bool> {
        let status: tendermint_rpc::endpoint::status::Response = self
            .tendermint_client
            .status()
            .await
            .context("failed to get Tendermint status")?;
        Ok(status.sync_info.catching_up)
    }
}
