// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
mod car;
mod client;
mod error;
mod manager;
mod manifest;
mod state;

/// The file name to export the CAR to.
const SNAPSHOT_FILE_NAME: &str = "snapshot.car";

/// The file name in snapshot directories that contains the manifest.
const MANIFEST_FILE_NAME: &str = "manifest.json";

/// Name of the subdirectory where `{idx}.part` files are stored within a snapshot.
const PARTS_DIR_NAME: &str = "parts";

pub use client::SnapshotClient;
pub use error::SnapshotError;
pub use manager::{SnapshotManager, SnapshotParams};
pub use manifest::SnapshotManifest;
pub use state::SnapshotItem;
