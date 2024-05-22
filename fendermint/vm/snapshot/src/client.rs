// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, sync::Arc, time::SystemTime};

use async_stm::{abort, Stm, StmResult, TVar};
use fendermint_vm_interpreter::fvm::state::{
    snapshot::{BlockHeight, SnapshotVersion},
    FvmStateParams,
};

use crate::{
    manifest,
    state::{SnapshotDownload, SnapshotState},
    SnapshotError, SnapshotItem, SnapshotManifest, MANIFEST_FILE_NAME,
};

/// Interface to snapshot state for the application.
#[derive(Clone)]
pub struct SnapshotClient {
    download_dir: PathBuf,
    /// The client will only notify the manager of snapshottable heights.
    snapshot_interval: BlockHeight,
    state: SnapshotState,
}

impl SnapshotClient {
    pub fn new(
        download_dir: PathBuf,
        snapshot_interval: BlockHeight,
        state: SnapshotState,
    ) -> Self {
        Self {
            download_dir,
            snapshot_interval,
            state,
        }
    }
    /// Set the latest block state parameters and notify the manager.
    ///
    /// Call this with the block height where the `app_hash` in the block reflects the
    /// state in the parameters, that is, the in the *next* block.
    pub fn notify(&self, block_height: BlockHeight, state_params: FvmStateParams) -> Stm<()> {
        if block_height % self.snapshot_interval == 0 {
            self.state
                .latest_params
                .write(Some((state_params, block_height)))?;
        }
        Ok(())
    }

    /// List completed snapshots.
    pub fn list_snapshots(&self) -> Stm<im::Vector<SnapshotItem>> {
        self.state.snapshots.read_clone()
    }

    /// Try to find a snapshot, if it still exists.
    ///
    /// If found, mark it as accessed, so that it doesn't get purged while likely to be requested or read from disk.
    pub fn access_snapshot(
        &self,
        block_height: BlockHeight,
        version: SnapshotVersion,
    ) -> Stm<Option<SnapshotItem>> {
        let mut snapshots = self.state.snapshots.read_clone()?;
        let mut snapshot = None;
        for s in snapshots.iter_mut() {
            if s.manifest.block_height == block_height && s.manifest.version == version {
                s.last_access = SystemTime::now();
                snapshot = Some(s.clone());
                break;
            }
        }
        if snapshot.is_some() {
            self.state.snapshots.write(snapshots)?;
        }
        Ok(snapshot)
    }

    /// If the offered snapshot is accepted, we create a temporary directory to hold the chunks
    /// and remember it as our current snapshot being downloaded.
    pub fn offer_snapshot(&self, manifest: SnapshotManifest) -> StmResult<PathBuf, SnapshotError> {
        if manifest.version != 1 {
            abort(SnapshotError::IncompatibleVersion(manifest.version))
        } else {
            match tempfile::tempdir_in(&self.download_dir) {
                Ok(dir) => {
                    // Save the manifest into the temp directory;
                    // that way we can always see on the file system what's happening.
                    let json = match serde_json::to_string_pretty(&manifest)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                    {
                        Ok(json) => json,
                        Err(e) => return abort(SnapshotError::from(e)),
                    };

                    let download_path: PathBuf = dir.path().into();
                    let download = SnapshotDownload {
                        manifest,
                        download_dir: Arc::new(dir),
                        next_index: TVar::new(0),
                    };

                    // Create a `parts` sub-directory for the chunks.
                    if let Err(e) = std::fs::create_dir(download.parts_dir()) {
                        return abort(SnapshotError::from(e));
                    };

                    if let Err(e) = std::fs::write(download_path.join(MANIFEST_FILE_NAME), json) {
                        return abort(SnapshotError::from(e));
                    }

                    self.state.current_download.write(Some(download))?;

                    Ok(download_path)
                }
                Err(e) => abort(SnapshotError::from(e))?,
            }
        }
    }

    /// Take a chunk sent to us by a remote peer. This is our chance to validate chunks on the fly.
    ///
    /// Returns `None` while there are more chunks to download and `Some` when all
    /// the chunks have been received and basic file integrity validated.
    ///
    /// Then we can import the snapshot into the blockstore separately.
    pub fn save_chunk(
        &self,
        index: u32,
        contents: Vec<u8>,
    ) -> StmResult<Option<SnapshotItem>, SnapshotError> {
        if let Some(cd) = self.state.current_download.read()?.as_ref() {
            let next_index = cd.next_index.read_clone()?;
            if index != next_index {
                abort(SnapshotError::UnexpectedChunk(next_index, index))
            } else {
                let part_path = cd.parts_dir().join(format!("{}.part", index));

                // We are doing IO inside the STM transaction, but that's okay because there is no contention on the download.
                match std::fs::write(part_path, contents) {
                    Ok(()) => {
                        let next_index = index + 1;
                        cd.next_index.write(next_index)?;

                        if next_index == cd.manifest.chunks {
                            // Verify the checksum then load the snapshot and remove the current download from memory.
                            match manifest::parts_checksum(cd.parts_dir()) {
                                Ok(checksum) => {
                                    if checksum == cd.manifest.checksum {
                                        let item = SnapshotItem::new(
                                            cd.download_dir.path().into(),
                                            cd.manifest.clone(),
                                        );
                                        Ok(Some(item))
                                    } else {
                                        abort(SnapshotError::WrongChecksum(
                                            cd.manifest.checksum,
                                            checksum,
                                        ))
                                    }
                                }
                                Err(e) => abort(SnapshotError::IoError(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    e.to_string(),
                                ))),
                            }
                        } else {
                            Ok(None)
                        }
                    }
                    Err(e) => {
                        // If we failed to save the data to disk we can return an error that will cause all snapshots to be aborted.
                        // There is no point trying to clear download from the state here because if we `abort` then all changes will be dropped.
                        abort(SnapshotError::from(e))
                    }
                }
            }
        } else {
            abort(SnapshotError::NoDownload)
        }
    }
}
