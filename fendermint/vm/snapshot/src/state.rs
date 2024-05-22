// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fs::File, io, path::PathBuf, sync::Arc, time::SystemTime};

use anyhow::{bail, Context};
use async_stm::TVar;
use fendermint_vm_interpreter::fvm::state::snapshot::{BlockStateParams, Snapshot};
use fvm_ipld_blockstore::Blockstore;
use tempfile::TempDir;

use crate::{
    manifest::{self, SnapshotManifest},
    PARTS_DIR_NAME, SNAPSHOT_FILE_NAME,
};

/// State of snapshots, including the list of available completed ones
/// and the next eligible height.
#[derive(Clone)]
pub struct SnapshotState {
    /// Completed snapshots.
    pub snapshots: TVar<im::Vector<SnapshotItem>>,
    /// The latest state parameters at a snapshottable height.
    pub latest_params: TVar<Option<BlockStateParams>>,
    /// The latest snapshot offered, which CometBFT is downloading and feeding to us.
    pub current_download: TVar<Option<SnapshotDownload>>,
}

impl SnapshotState {
    pub fn new(snapshots: Vec<SnapshotItem>) -> Self {
        Self {
            snapshots: TVar::new(snapshots.into()),
            // Start with nothing to snapshot until we are notified about a new height.
            // We could also look back to find the latest height we should have snapshotted.
            latest_params: TVar::new(None),
            current_download: TVar::new(None),
        }
    }
}

/// A snapshot directory and its manifest.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SnapshotItem {
    /// Directory containing this snapshot, ie. the manifest and the parts.
    pub snapshot_dir: PathBuf,
    /// Parsed `manifest.json` contents.
    pub manifest: SnapshotManifest,
    /// Last time a peer asked for a chunk from this snapshot.
    pub last_access: SystemTime,
}

impl SnapshotItem {
    pub fn new(snapshot_dir: PathBuf, manifest: SnapshotManifest) -> Self {
        Self {
            snapshot_dir,
            manifest,
            last_access: SystemTime::UNIX_EPOCH,
        }
    }

    fn parts_dir(&self) -> PathBuf {
        self.snapshot_dir.join(PARTS_DIR_NAME)
    }

    /// Load the data from disk.
    ///
    /// Returns an error if the chunk isn't within range or if the file doesn't exist any more.
    pub fn load_chunk(&self, chunk: u32) -> anyhow::Result<Vec<u8>> {
        if chunk >= self.manifest.chunks {
            bail!(
                "cannot load chunk {chunk}; only have {} in the snapshot",
                self.manifest.chunks
            );
        }
        let chunk_file = self.parts_dir().join(format!("{chunk}.part"));

        let content = std::fs::read(&chunk_file)
            .with_context(|| format!("failed to read chunk {}", chunk_file.to_string_lossy()))?;

        Ok(content)
    }

    /// Import a snapshot into the blockstore.
    pub async fn import<BS>(&self, store: BS, validate: bool) -> anyhow::Result<Snapshot<BS>>
    where
        BS: Blockstore + Send + Clone + 'static,
    {
        let parts =
            manifest::list_parts(self.parts_dir()).context("failed to list snapshot parts")?;

        // 1. Restore the snapshots into a complete `snapshot.car` file.
        let car_path = self.snapshot_dir.join(SNAPSHOT_FILE_NAME);
        let mut car_file = File::create(&car_path).context("failed to create CAR file")?;

        for part in parts {
            let mut part_file = File::open(&part).with_context(|| {
                format!("failed to open snapshot part {}", part.to_string_lossy())
            })?;

            io::copy(&mut part_file, &mut car_file)?;
        }

        // 2. Import the contents.
        let result = Snapshot::read_car(&car_path, store, validate).await;

        // 3. Remove the restored file.
        std::fs::remove_file(&car_path).context("failed to remove CAR file")?;

        // If the import failed, or it fails to validate, it will leave unwanted data in the blockstore.
        //
        // We could do the import into a namespace which is separate from the state store, and move the data
        // if everything we see what successful, but it would need more database API exposed that we don't
        // currently have access to. At the moment our best bet to remove the data is to implement garbage
        // collection - if the CIDs are unreachable through state roots, they will be removed.
        //
        // Another thing worth noting is that the `Snapshot` imports synthetic records into the blockstore
        // that did not exist in the original: the metadata, an some technical constructs that point at
        // the real data and store application state (which is verfied below). It's not easy to get rid
        // of these: the `Blockstore` doesn't allow us to delete CIDs, and the `Snapshot` doesn't readily
        // expose what the CIDs of the extra records were. Our other option would be to load the data
        // into a staging area (see above) and then walk the DAG and only load what is reachable from
        // the state root.
        //
        // Inserting CIDs into the state store which did not exist in the original seem like a vector
        // of attack that could be used to cause consensus failure: if the attacker deployed a contract
        // that looked up a CID that validators who imported a snapshot have, but others don't, that
        // would cause a fork. However,  his is why the FVM doesn't currently allow the deployment of
        // user defined Wasm actors: the FEVM actors do not allow the lookup of arbitrary CIDs, so they
        // are safe, while Wasm actors with direct access to the IPLD SDK methods would be vulnerable.
        // Once the FVM implements the "reachability analysis" feature, it won't matter if we have an
        // extra record or not.
        //
        // Actually a very similar situation arises with garbage collection: since the length of history
        // is configurable, whether some CIDs are (still) present or not depends on how the validator
        // configured their nodes, and cannot be allowed to cause a failure.
        let snapshot = result.context("failed to import the snapshot into the blockstore")?;

        // 4. See if we actually imported what we thought we would.
        if validate {
            match snapshot {
                Snapshot::V1(ref snapshot) => {
                    if snapshot.block_height() != self.manifest.block_height {
                        bail!(
                            "invalid snapshot block height; expected {}, imported {}",
                            self.manifest.block_height,
                            snapshot.block_height()
                        );
                    }
                    if *snapshot.state_params() != self.manifest.state_params {
                        bail!(
                            "invalid state params; expected {:?}, imported {:?}",
                            self.manifest.state_params,
                            snapshot.state_params()
                        )
                    }
                }
            }
        }

        Ok(snapshot)
    }
}

/// An ongoing, incomplete download of a snapshot.
#[derive(Clone)]
pub struct SnapshotDownload {
    pub manifest: SnapshotManifest,
    // Temporary download directory. Removed when this download is dropped.
    pub download_dir: Arc<TempDir>,
    // Next expected chunk index.
    pub next_index: TVar<u32>,
}

impl SnapshotDownload {
    pub fn parts_dir(&self) -> PathBuf {
        self.download_dir.path().join(PARTS_DIR_NAME)
    }
}

#[cfg(feature = "arb")]
mod arb {
    use std::{path::PathBuf, time::SystemTime};

    use super::{SnapshotItem, SnapshotManifest};

    impl quickcheck::Arbitrary for SnapshotItem {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self {
                manifest: SnapshotManifest::arbitrary(g),
                snapshot_dir: PathBuf::arbitrary(g),
                last_access: SystemTime::arbitrary(g),
            }
        }
    }
}
