// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::manifest::{file_checksum, list_manifests, write_manifest, SnapshotManifest};
use crate::state::SnapshotState;
use crate::{car, SnapshotClient, SnapshotItem, PARTS_DIR_NAME, SNAPSHOT_FILE_NAME};
use anyhow::Context;
use async_stm::{atomically, retry, TVar};
use fendermint_vm_interpreter::fvm::state::snapshot::{BlockHeight, Snapshot};
use fendermint_vm_interpreter::fvm::state::FvmStateParams;
use fvm_ipld_blockstore::Blockstore;
use tendermint_rpc::Client;

pub struct SnapshotParams {
    /// Location to store completed snapshots.
    pub snapshots_dir: PathBuf,
    pub download_dir: PathBuf,
    pub block_interval: BlockHeight,
    /// Target size in bytes for snapshot chunks.
    pub chunk_size: usize,
    /// Number of snapshots to keep.
    ///
    /// 0 means unlimited.
    pub hist_size: usize,
    /// Time to hold on from purging a snapshot after a remote client
    /// asked for a chunk from it.
    pub last_access_hold: Duration,
    /// How often to check CometBFT whether it has finished syncing.
    pub sync_poll_interval: Duration,
}

/// Create snapshots at regular block intervals.
pub struct SnapshotManager<BS> {
    store: BS,
    snapshots_dir: PathBuf,
    chunk_size: usize,
    hist_size: usize,
    last_access_hold: Duration,
    sync_poll_interval: Duration,
    /// Shared state of snapshots.
    state: SnapshotState,
    /// Indicate whether CometBFT has finished syncing with the chain,
    /// so that we can skip snapshotting old states while catching up.
    is_syncing: TVar<bool>,
}

impl<BS> SnapshotManager<BS>
where
    BS: Blockstore + Clone + Send + Sync + 'static,
{
    /// Create a new manager.
    pub fn new(store: BS, params: SnapshotParams) -> anyhow::Result<(Self, SnapshotClient)> {
        // Make sure the target directory exists.
        std::fs::create_dir_all(&params.snapshots_dir)
            .context("failed to create snapshots directory")?;

        let snapshot_items =
            list_manifests(&params.snapshots_dir).context("failed to list manifests")?;

        let state = SnapshotState::new(snapshot_items);

        let manager: SnapshotManager<BS> = Self {
            store,
            snapshots_dir: params.snapshots_dir,
            chunk_size: params.chunk_size,
            hist_size: params.hist_size,
            last_access_hold: params.last_access_hold,
            sync_poll_interval: params.sync_poll_interval,
            state: state.clone(),
            // Assume we are syncing until we can determine otherwise.
            is_syncing: TVar::new(true),
        };

        let client = SnapshotClient::new(params.download_dir, params.block_interval, state);

        Ok((manager, client))
    }

    /// Produce snapshots.
    pub async fn run<C>(self, client: C)
    where
        C: Client + Send + Sync + 'static,
    {
        // Start a background poll to CometBFT.
        // We could just do this once and await here, but this way ostensibly CometBFT could be
        // restarted without Fendermint and go through another catch up.
        {
            if self.sync_poll_interval.is_zero() {
                atomically(|| self.is_syncing.write(false)).await;
            } else {
                let is_syncing = self.is_syncing.clone();
                let poll_interval = self.sync_poll_interval;
                tokio::spawn(async move {
                    poll_sync_status(client, is_syncing, poll_interval).await;
                });
            }
        }

        let mut last_params = None;
        loop {
            let (state_params, block_height) = atomically(|| {
                // Check the current sync status. We could just query the API, but then we wouldn't
                // be notified when we finally reach the end, and we'd only snapshot the next height,
                // not the last one as soon as the chain is caught up.
                if *self.is_syncing.read()? {
                    retry()?;
                }

                match self.state.latest_params.read()?.as_ref() {
                    None => retry()?,
                    unchanged if *unchanged == last_params => retry()?,
                    Some(new_params) => Ok(new_params.clone()),
                }
            })
            .await;

            match self
                .create_snapshot(block_height, state_params.clone())
                .await
            {
                Ok(item) => {
                    tracing::info!(
                        snapshot = item.snapshot_dir.to_string_lossy().to_string(),
                        block_height,
                        chunks_count = item.manifest.chunks,
                        snapshot_size = item.manifest.size,
                        "exported snapshot"
                    );
                    // Add the snapshot to the in-memory records.
                    atomically(|| {
                        self.state
                            .snapshots
                            .modify_mut(|items| items.push_back(item.clone()))
                    })
                    .await;
                }
                Err(e) => {
                    tracing::warn!(error =? e, block_height, "failed to create snapshot");
                }
            }

            // Delete old snapshots.
            self.prune_history().await;

            last_params = Some((state_params, block_height));
        }
    }

    /// Remove snapshot directories if we have more than the desired history size.
    async fn prune_history(&self) {
        if self.hist_size == 0 {
            return;
        }

        let removables = atomically(|| {
            self.state.snapshots.modify_mut(|snapshots| {
                let mut removables = Vec::new();
                while snapshots.len() > self.hist_size {
                    // Stop at the first snapshot that was accessed recently.
                    if let Some(last_access) =
                        snapshots.head().and_then(|s| s.last_access.elapsed().ok())
                    {
                        if last_access <= self.last_access_hold {
                            break;
                        }
                    }
                    if let Some(snapshot) = snapshots.pop_front() {
                        removables.push(snapshot);
                    } else {
                        break;
                    }
                }
                removables
            })
        })
        .await;

        for r in removables {
            let snapshot_dir = r.snapshot_dir.to_string_lossy().to_string();
            if let Err(e) = std::fs::remove_dir_all(&r.snapshot_dir) {
                tracing::error!(error =? e, snapshot_dir, "failed to remove snapshot");
            } else {
                tracing::info!(snapshot_dir, "removed snapshot");
            }
        }
    }

    /// Export a snapshot to a temporary file, then copy it to the snapshot directory.
    async fn create_snapshot(
        &self,
        block_height: BlockHeight,
        state_params: FvmStateParams,
    ) -> anyhow::Result<SnapshotItem> {
        let snapshot = Snapshot::new(self.store.clone(), state_params.clone(), block_height)
            .context("failed to create snapshot")?;

        let snapshot_version = snapshot.version();
        let snapshot_name = format!("snapshot-{block_height}");
        let temp_dir = tempfile::Builder::new()
            .prefix(&snapshot_name)
            .tempdir()
            .context("failed to create temp dir for snapshot")?;

        let snapshot_path = temp_dir.path().join(SNAPSHOT_FILE_NAME);
        let checksum_path = temp_dir.path().join(format!("{PARTS_DIR_NAME}.sha256"));
        let parts_path = temp_dir.path().join(PARTS_DIR_NAME);

        // TODO: See if we can reuse the contents of an existing CAR file.

        tracing::debug!(
            block_height,
            path = snapshot_path.to_string_lossy().to_string(),
            "exporting snapshot..."
        );

        // Export the state to a CAR file.
        snapshot
            .write_car(&snapshot_path)
            .await
            .context("failed to write CAR file")?;

        let snapshot_size = std::fs::metadata(&snapshot_path)
            .context("failed to get snapshot metadata")?
            .len() as usize;

        // Create a checksum over the CAR file.
        let checksum_bytes = file_checksum(&snapshot_path).context("failed to compute checksum")?;

        std::fs::write(&checksum_path, checksum_bytes.to_string())
            .context("failed to write checksum file")?;

        // Create a directory for the parts.
        std::fs::create_dir(&parts_path).context("failed to create parts dir")?;

        // Split the CAR file into chunks.
        // They can be listed in the right order with e.g. `ls | sort -n`
        // Alternatively we could pad them with zeroes based on the original file size and the chunk size,
        // but this way it will be easier to return them based on a numeric index.
        let chunks_count = car::split(&snapshot_path, &parts_path, self.chunk_size, |idx| {
            format!("{idx}.part")
        })
        .await
        .context("failed to split CAR into chunks")?;

        // Create and export a manifest that we can easily look up.
        let manifest = SnapshotManifest {
            block_height,
            size: snapshot_size as u64,
            chunks: chunks_count as u32,
            checksum: checksum_bytes,
            state_params,
            version: snapshot_version,
        };
        let _ = write_manifest(temp_dir.path(), &manifest).context("failed to export manifest")?;

        let snapshots_dir = self.snapshots_dir.join(&snapshot_name);
        move_or_copy(temp_dir.path(), &snapshots_dir).context("failed to move snapshot")?;

        Ok(SnapshotItem::new(snapshots_dir, manifest))
    }
}

/// Periodically ask CometBFT if it has caught up with the chain.
async fn poll_sync_status<C>(client: C, is_syncing: TVar<bool>, poll_interval: Duration)
where
    C: Client + Send + Sync + 'static,
{
    loop {
        match client.status().await {
            Ok(status) => {
                let catching_up = status.sync_info.catching_up;

                atomically(|| {
                    if *is_syncing.read()? != catching_up {
                        is_syncing.write(catching_up)?;
                    }
                    Ok(())
                })
                .await;
            }
            Err(e) => {
                tracing::warn!(error =? e, "failed to poll CometBFT sync status");
            }
        }
        tokio::time::sleep(poll_interval).await;
    }
}

/// Try to move the entire snapshot directory to its final place,
/// then remove the snapshot file, keeping only the parts.
///
/// If that fails, for example because it would be moving between a
/// Docker container's temporary directory to the host mounted volume,
/// then fall back to copying.
fn move_or_copy(from: &Path, to: &Path) -> anyhow::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        // Delete the big CAR file - keep the only the parts.
        std::fs::remove_file(to.join(SNAPSHOT_FILE_NAME)).context("failed to remove CAR file")?;
    } else {
        dircpy::CopyBuilder::new(from, to)
            .with_exclude_filter(SNAPSHOT_FILE_NAME)
            .run()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_stm::{atomically, retry};
    use fendermint_vm_genesis::Genesis;
    use fendermint_vm_interpreter::{
        fvm::{
            bundle::{bundle_path, contracts_path, custom_actors_bundle_path},
            state::{snapshot::Snapshot, FvmGenesisState, FvmStateParams},
            store::memory::MemoryBlockstore,
            upgrades::UpgradeScheduler,
            FvmMessageInterpreter,
        },
        GenesisInterpreter,
    };
    use fvm::engine::MultiEngine;
    use quickcheck::Arbitrary;

    use crate::{manager::SnapshotParams, manifest, PARTS_DIR_NAME};

    use super::SnapshotManager;

    // Initialise genesis and export it directly to see if it works.
    #[tokio::test]
    async fn create_snapshots_directly() {
        let (state_params, store) = init_genesis().await;
        let snapshot = Snapshot::new(store, state_params, 0).expect("failed to create snapshot");
        let tmp_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        snapshot
            .write_car(&tmp_path)
            .await
            .expect("failed to write snapshot");
    }

    // Initialise genesis, create a snapshot manager, export a snapshot, create another manager, list snapshots.
    // Don't forget to run this with `--release` beause of Wasm.
    #[tokio::test]
    async fn create_snapshot_with_manager() {
        let (state_params, store) = init_genesis().await;

        // Now we have one store initialized with genesis, let's create a manager and snapshot it.
        let snapshots_dir = tempfile::tempdir().expect("failed to create tmp dir");
        let download_dir = tempfile::tempdir().expect("failed to create tmp dir");

        // Not polling because it's cumbersome to mock it.
        let never_poll_sync = Duration::ZERO;
        let never_poll_client = mock_client();

        let (snapshot_manager, snapshot_client) = SnapshotManager::new(
            store.clone(),
            SnapshotParams {
                snapshots_dir: snapshots_dir.path().into(),
                download_dir: download_dir.path().into(),
                block_interval: 1,
                chunk_size: 10000,
                hist_size: 1,
                last_access_hold: Duration::ZERO,
                sync_poll_interval: never_poll_sync,
            },
        )
        .expect("failed to create snapshot manager");

        // Start the manager in the background
        tokio::spawn(async move { snapshot_manager.run(never_poll_client).await });

        // Make sure we have no snapshots currently.
        let snapshots = atomically(|| snapshot_client.list_snapshots()).await;
        assert!(snapshots.is_empty());

        // Notify about snapshottable height.
        atomically(|| snapshot_client.notify(0, state_params.clone())).await;

        // Wait for the new snapshot to appear in memory.
        let snapshots = tokio::time::timeout(
            Duration::from_secs(10),
            atomically(|| {
                let snapshots = snapshot_client.list_snapshots()?;
                if snapshots.is_empty() {
                    retry()
                } else {
                    Ok(snapshots)
                }
            }),
        )
        .await
        .expect("failed to export snapshot");

        assert_eq!(snapshots.len(), 1);

        let snapshot = snapshots.into_iter().next().unwrap();
        assert!(snapshot.manifest.chunks > 1);
        assert_eq!(snapshot.manifest.block_height, 0);
        assert_eq!(snapshot.manifest.state_params, state_params);
        assert_eq!(
            snapshot.snapshot_dir.as_path(),
            snapshots_dir.path().join("snapshot-0")
        );

        let _ = std::fs::File::open(snapshot.snapshot_dir.join("manifest.json"))
            .expect("manifests file exists");

        let snapshots = manifest::list_manifests(snapshots_dir.path()).unwrap();

        assert_eq!(snapshots.len(), 1, "can list manifests");
        assert_eq!(snapshots[0], snapshot);

        let checksum =
            manifest::parts_checksum(snapshot.snapshot_dir.as_path().join(PARTS_DIR_NAME))
                .expect("parts checksum can be calculated");

        assert_eq!(
            checksum, snapshot.manifest.checksum,
            "checksum should match"
        );

        // Create a new manager instance
        let (_, new_client) = SnapshotManager::new(
            store,
            SnapshotParams {
                snapshots_dir: snapshots_dir.path().into(),
                download_dir: download_dir.path().into(),
                block_interval: 1,
                chunk_size: 10000,
                hist_size: 1,
                last_access_hold: Duration::ZERO,
                sync_poll_interval: never_poll_sync,
            },
        )
        .expect("failed to create snapshot manager");

        let snapshots = atomically(|| new_client.list_snapshots()).await;
        assert!(!snapshots.is_empty(), "loads manifests on start");
    }

    async fn init_genesis() -> (FvmStateParams, MemoryBlockstore) {
        let mut g = quickcheck::Gen::new(5);
        let genesis = Genesis::arbitrary(&mut g);

        let bundle = std::fs::read(bundle_path()).expect("failed to read bundle");
        let custom_actors_bundle = std::fs::read(custom_actors_bundle_path())
            .expect("failed to read custom actors bundle");
        let multi_engine = Arc::new(MultiEngine::default());

        let store = MemoryBlockstore::new();
        let state =
            FvmGenesisState::new(store.clone(), multi_engine, &bundle, &custom_actors_bundle)
                .await
                .expect("failed to create state");

        let interpreter = FvmMessageInterpreter::new(
            mock_client(),
            None,
            contracts_path(),
            1.05,
            1.05,
            false,
            UpgradeScheduler::new(),
        );

        let (state, out) = interpreter
            .init(state, genesis)
            .await
            .expect("failed to init genesis");

        let state_root = state.commit().expect("failed to commit");

        let state_params = FvmStateParams {
            state_root,
            timestamp: out.timestamp,
            network_version: out.network_version,
            base_fee: out.base_fee,
            circ_supply: out.circ_supply,
            chain_id: out.chain_id.into(),
            power_scale: out.power_scale,
            app_version: 0,
        };

        (state_params, store)
    }

    fn mock_client() -> tendermint_rpc::MockClient<tendermint_rpc::MockRequestMethodMatcher> {
        tendermint_rpc::MockClient::new(tendermint_rpc::MockRequestMethodMatcher::default()).0
    }
}
