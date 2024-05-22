// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Bottom up checkpoint manager

use crate::config::Subnet;
use crate::manager::{BottomUpCheckpointRelayer, EthSubnetManager};
use anyhow::{anyhow, Result};
use futures_util::future::try_join_all;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use ipc_api::checkpoint::{BottomUpCheckpointBundle, QuorumReachedEvent};
use ipc_wallet::{EthKeyAddress, PersistentKeyStore};
use std::cmp::max;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Semaphore;

/// Tracks the config required for bottom up checkpoint submissions
/// parent/child subnet and checkpoint period.
pub struct CheckpointConfig {
    parent: Subnet,
    child: Subnet,
    period: ChainEpoch,
}

/// Manages the submission of bottom up checkpoint. It checks if the submitter has already
/// submitted in the `last_checkpoint_height`, if not, it will submit the checkpoint at that height.
/// Then it will submit at the next submission height for the new checkpoint.
pub struct BottomUpCheckpointManager<T> {
    metadata: CheckpointConfig,
    parent_handler: Arc<T>,
    child_handler: T,
    /// The number of blocks away from the chain head that is considered final
    finalization_blocks: ChainEpoch,
    submission_semaphore: Arc<Semaphore>,
}

impl<T: BottomUpCheckpointRelayer> BottomUpCheckpointManager<T> {
    pub async fn new(
        parent: Subnet,
        child: Subnet,
        parent_handler: T,
        child_handler: T,
        max_parallelism: usize,
    ) -> Result<Self> {
        let period = parent_handler
            .checkpoint_period(&child.id)
            .await
            .map_err(|e| anyhow!("cannot get bottom up checkpoint period: {e}"))?;
        Ok(Self {
            metadata: CheckpointConfig {
                parent,
                child,
                period,
            },
            parent_handler: Arc::new(parent_handler),
            child_handler,
            finalization_blocks: 0,
            submission_semaphore: Arc::new(Semaphore::new(max_parallelism)),
        })
    }

    pub fn with_finalization_blocks(mut self, finalization_blocks: ChainEpoch) -> Self {
        self.finalization_blocks = finalization_blocks;
        self
    }
}

impl BottomUpCheckpointManager<EthSubnetManager> {
    pub async fn new_evm_manager(
        parent: Subnet,
        child: Subnet,
        keystore: Arc<RwLock<PersistentKeyStore<EthKeyAddress>>>,
        max_parallelism: usize,
    ) -> Result<Self> {
        let parent_handler =
            EthSubnetManager::from_subnet_with_wallet_store(&parent, Some(keystore.clone()))?;
        let child_handler =
            EthSubnetManager::from_subnet_with_wallet_store(&child, Some(keystore))?;
        Self::new(
            parent,
            child,
            parent_handler,
            child_handler,
            max_parallelism,
        )
        .await
    }
}

impl<T: BottomUpCheckpointRelayer> Display for BottomUpCheckpointManager<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bottom-up relayer, parent: {:}, child: {:}",
            self.metadata.parent.id, self.metadata.child.id
        )
    }
}

impl<T: BottomUpCheckpointRelayer + Send + Sync + 'static> BottomUpCheckpointManager<T> {
    /// Getter for the parent subnet this checkpoint manager is handling
    pub fn parent_subnet(&self) -> &Subnet {
        &self.metadata.parent
    }

    /// Getter for the target subnet this checkpoint manager is handling
    pub fn child_subnet(&self) -> &Subnet {
        &self.metadata.child
    }

    /// The checkpoint period that the current manager is submitting upon
    pub fn checkpoint_period(&self) -> ChainEpoch {
        self.metadata.period
    }

    /// Run the bottom up checkpoint submission daemon in the foreground
    pub async fn run(self, submitter: Address, submission_interval: Duration) {
        tracing::info!("launching {self} for {submitter}");

        loop {
            if let Err(e) = self.submit_next_epoch(submitter).await {
                tracing::error!("cannot submit checkpoint for submitter: {submitter} due to {e}");
            }
            tokio::time::sleep(submission_interval).await;
        }
    }

    /// Checks if the relayer has already submitted at the next submission epoch, if not it submits it.
    async fn submit_next_epoch(&self, submitter: Address) -> Result<()> {
        let last_checkpoint_epoch = self
            .parent_handler
            .last_bottom_up_checkpoint_height(&self.metadata.child.id)
            .await
            .map_err(|e| {
                anyhow!("cannot obtain the last bottom up checkpoint height due to: {e:}")
            })?;
        tracing::info!("last submission height: {last_checkpoint_epoch}");

        let current_height = self.child_handler.current_epoch().await?;
        let finalized_height = max(1, current_height - self.finalization_blocks);

        tracing::debug!("last submission height: {last_checkpoint_epoch}, current height: {current_height}, finalized_height: {finalized_height}");

        if finalized_height <= last_checkpoint_epoch {
            return Ok(());
        }

        let start = last_checkpoint_epoch + 1;
        tracing::debug!(
            "start querying quorum reached events from : {start} to {finalized_height}"
        );

        let mut count = 0;
        let mut all_submit_tasks = vec![];

        for h in start..=finalized_height {
            let events = self.child_handler.quorum_reached_events(h).await?;
            if events.is_empty() {
                tracing::debug!("no reached events at height : {h}");
                continue;
            }

            tracing::debug!("found reached events at height : {h}");

            for event in events {
                // Note that the event will be emitted later than the checkpoint height.
                // For example, if the checkpoint height is 400 but it's actually created
                // in fendermint at height 403. This means the event.height == 400 which is
                // already committed.
                if event.height <= last_checkpoint_epoch {
                    tracing::debug!("event height already committed: {}", event.height);
                    continue;
                }

                let bundle = self
                    .child_handler
                    .checkpoint_bundle_at(event.height)
                    .await?
                    .ok_or_else(|| {
                        anyhow!(
                            "expected checkpoint at height {} but none found",
                            event.height
                        )
                    })?;

                log::debug!("bottom up bundle: {bundle:?}");

                // We support parallel checkpoint submission using FIFO order with a limited parallelism (controlled by
                // the size of submission_semaphore).
                // We need to acquire a permit (from a limited permit pool) before submitting a checkpoint.
                // We may wait here until a permit is available.
                let parent_handler_clone = Arc::clone(&self.parent_handler);
                let submission_permit = self
                    .submission_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .unwrap();
                all_submit_tasks.push(tokio::task::spawn(async move {
                    let height = event.height;
                    let result =
                        Self::submit_checkpoint(parent_handler_clone, submitter, bundle, event)
                            .await
                            .inspect_err(|err| {
                                tracing::error!(
                                    "Fail to submit checkpoint at height {height}: {err}"
                                );
                            });
                    drop(submission_permit);
                    result
                }));

                count += 1;
                tracing::debug!("This round has asynchronously submitted {count} checkpoints",);
            }
        }

        tracing::debug!("Waiting for all submissions to finish");
        // Return error if any of the submit task failed.
        try_join_all(all_submit_tasks).await?;

        Ok(())
    }

    async fn submit_checkpoint(
        parent_handler: Arc<T>,
        submitter: Address,
        bundle: BottomUpCheckpointBundle,
        event: QuorumReachedEvent,
    ) -> Result<(), anyhow::Error> {
        let epoch = parent_handler
            .submit_checkpoint(
                &submitter,
                bundle.checkpoint,
                bundle.signatures,
                bundle.signatories,
            )
            .await
            .map_err(|e| {
                anyhow!(
                    "cannot submit bottom up checkpoint at height {} due to: {e}",
                    event.height
                )
            })?;

        tracing::info!(
            "submitted bottom up checkpoint({}) in parent at height {}",
            event.height,
            epoch
        );
        Ok(())
    }
}
