// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};

use anyhow::Context;
use fendermint_vm_interpreter::fvm::state::{
    snapshot::{BlockHeight, SnapshotVersion},
    FvmStateParams,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{SnapshotItem, MANIFEST_FILE_NAME};

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct SnapshotManifest {
    /// Block height where the snapshot was taken.
    pub block_height: BlockHeight,
    /// Snapshot size in bytes.
    pub size: u64,
    /// Number of chunks in the snapshot.
    pub chunks: u32,
    /// SHA2 hash of the snapshot contents.
    ///
    /// Using a [tendermint::Hash] type because it has nice formatting in JSON.
    pub checksum: tendermint::Hash,
    /// The FVM parameters at the time of the snapshot,
    /// which are also in the CAR file, but it might be
    /// useful to see. It is annotated for human readability.
    pub state_params: FvmStateParams,
    /// Snapshot format version
    pub version: SnapshotVersion,
}

/// Save a manifest along with the other snapshot files into a snapshot specific directory.
pub fn write_manifest(
    snapshot_dir: impl AsRef<Path>,
    manifest: &SnapshotManifest,
) -> anyhow::Result<PathBuf> {
    let json =
        serde_json::to_string_pretty(&manifest).context("failed to convert manifest to JSON")?;

    let manifest_path = snapshot_dir.as_ref().join(MANIFEST_FILE_NAME);

    std::fs::write(&manifest_path, json).context("failed to write manifest file")?;

    Ok(manifest_path)
}

/// Collect all the manifests from a directory containing snapshot-directories, e.g.
/// `snapshots/snapshot-1/manifest.json` etc.
pub fn list_manifests(snapshot_dir: impl AsRef<Path>) -> anyhow::Result<Vec<SnapshotItem>> {
    let contents = std::fs::read_dir(snapshot_dir).context("failed to read snapshot directory")?;

    // Collect all manifest file paths.
    let mut manifests = Vec::new();
    for entry in contents {
        match entry {
            Ok(entry) => match entry.metadata() {
                Ok(metadata) => {
                    if metadata.is_dir() {
                        let manifest_path = entry.path().join(MANIFEST_FILE_NAME);
                        if manifest_path.exists() {
                            manifests.push((entry.path(), manifest_path))
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error =? e, "faulty entry metadata");
                }
            },
            Err(e) => {
                tracing::error!(error =? e, "faulty snapshot entry");
            }
        }
    }

    // Parse manifests
    let mut items = Vec::new();

    for (snapshot_dir, manifest) in manifests {
        let json = std::fs::read_to_string(&manifest).context("failed to open manifest")?;
        match serde_json::from_str(&json) {
            Ok(manifest) => items.push(SnapshotItem::new(snapshot_dir, manifest)),
            Err(e) => {
                tracing::error!(
                    manifest = manifest.to_string_lossy().to_string(),
                    error =? e,
                    "unable to parse snapshot manifest"
                );
            }
        }
    }

    // Order by oldest to newest.
    items.sort_by_key(|i| i.manifest.block_height);

    Ok(items)
}

/// Calculate the Sha256 checksum of a file.
pub fn file_checksum(path: impl AsRef<Path>) -> anyhow::Result<tendermint::Hash> {
    let mut file = std::fs::File::open(&path)?;
    let mut hasher = Sha256::new();
    let _ = std::io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize().into();
    Ok(tendermint::Hash::Sha256(hash))
}

/// Calculate the Sha256 checksum of all `{idx}.part` files in a directory.
pub fn parts_checksum(path: impl AsRef<Path>) -> anyhow::Result<tendermint::Hash> {
    let mut hasher = Sha256::new();

    let chunks = list_parts(path)?;

    for path in chunks {
        let mut file = std::fs::File::open(path).context("failed to open part")?;
        let _ = std::io::copy(&mut file, &mut hasher)?;
    }

    let hash = hasher.finalize().into();
    Ok(tendermint::Hash::Sha256(hash))
}

/// List all the `{idx}.part` files in a directory.
pub fn list_parts(path: impl AsRef<Path>) -> anyhow::Result<Vec<PathBuf>> {
    let mut chunks = std::fs::read_dir(path.as_ref())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "failed to collect parts in directory: {}",
                path.as_ref().to_string_lossy()
            )
        })?;

    chunks.retain(|item| {
        item.path()
            .extension()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default()
            == "part"
    });

    chunks.sort_by_cached_key(|item| {
        item.path()
            .file_stem()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
            .parse::<u32>()
            .expect("file part names are prefixed by index")
    });

    Ok(chunks.into_iter().map(|c| c.path()).collect())
}

#[cfg(feature = "arb")]
mod arb {

    use fendermint_testing::arb::{ArbCid, ArbTokenAmount};
    use fendermint_vm_core::{chainid, Timestamp};
    use fendermint_vm_interpreter::fvm::state::FvmStateParams;
    use fvm_shared::version::NetworkVersion;
    use quickcheck::Arbitrary;

    use super::SnapshotManifest;

    impl quickcheck::Arbitrary for SnapshotManifest {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let checksum: [u8; 32] = std::array::from_fn(|_| u8::arbitrary(g));

            Self {
                block_height: u32::arbitrary(g) as u64,
                size: Arbitrary::arbitrary(g),
                chunks: Arbitrary::arbitrary(g),
                checksum: tendermint::Hash::from_bytes(
                    tendermint::hash::Algorithm::Sha256,
                    &checksum,
                )
                .unwrap(),
                state_params: FvmStateParams {
                    state_root: ArbCid::arbitrary(g).0,
                    timestamp: Timestamp(Arbitrary::arbitrary(g)),
                    network_version: NetworkVersion::MAX,
                    base_fee: ArbTokenAmount::arbitrary(g).0,
                    circ_supply: ArbTokenAmount::arbitrary(g).0,
                    chain_id: chainid::from_str_hashed(String::arbitrary(g).as_str())
                        .unwrap()
                        .into(),
                    power_scale: *g.choose(&[-1, 0, 3]).unwrap(),
                    app_version: 0,
                },
                version: Arbitrary::arbitrary(g),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use cid::multihash::MultihashDigest;
    use tempfile::NamedTempFile;

    use crate::manifest::file_checksum;

    #[test]
    fn test_file_checksum() {
        let content = b"Hello Checksum!";

        let mut file = NamedTempFile::new().expect("new temp file");
        file.write_all(content).expect("write contents");
        let file_path = file.into_temp_path();
        let file_digest = file_checksum(file_path).expect("checksum");

        let content_digest = cid::multihash::Code::Sha2_256.digest(content);
        let content_digest = content_digest.digest();

        assert_eq!(file_digest.as_bytes(), content_digest)
    }
}
