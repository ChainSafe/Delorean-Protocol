// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{os::unix::fs::PermissionsExt, path::Path};

mod defaults;

use anyhow::Context;
pub use defaults::*;
use serde::{de::DeserializeOwned, Serialize};

/// Type family of all the things a [Materializer] can create.
///
/// Kept separate from the [Materializer] so that we can wrap one in another
/// and pass the same types along.
pub trait Materials {
    /// Represents the entire hierarchy of a testnet, e.g. a common docker network
    /// and directory on the file system. It has its own type so the materializer
    /// doesn't have to remember what it created for a testnet, and different
    /// testnets can be kept isolated from each other.
    type Network: Send + Sync;
    /// Capture where the IPC stack (the gateway and the registry) has been deployed on a subnet.
    /// These are the details which normally go into the `ipc-cli` configuration files.
    type Deployment: Sync + Send;
    /// Represents an account identity, typically a key-value pair.
    type Account: Ord + Sync + Send;
    /// Represents the genesis.json file (can be a file location, or a model).
    type Genesis: Sync + Send;
    /// The address of a dynamically created subnet.
    type Subnet: Sync + Send;
    /// The handle to a node; could be a (set of) docker container(s) or remote addresses.
    type Node: Sync + Send;
    /// The handle to a relayer process.
    type Relayer: Sync + Send;
}

/// Write some content to a file.
///
/// It will create all the directories along the path.
pub fn export(
    output_dir: impl AsRef<Path>,
    name: &str,
    ext: &str,
    contents: impl AsRef<str>,
) -> anyhow::Result<()> {
    let file_name = if ext.is_empty() {
        name.into()
    } else {
        format!("{name}.{ext}")
    };

    let dir_path = output_dir.as_ref();
    let file_path = dir_path.join(file_name);

    export_file(file_path, contents)
}
/// Export text to a file.
pub fn export_file(file_path: impl AsRef<Path>, contents: impl AsRef<str>) -> anyhow::Result<()> {
    if let Some(dir_path) = file_path.as_ref().parent() {
        if !dir_path.exists() {
            std::fs::create_dir_all(dir_path).with_context(|| {
                format!("failed to create directory {}", dir_path.to_string_lossy())
            })?;
        }
    }

    std::fs::write(&file_path, contents.as_ref()).with_context(|| {
        format!(
            "failed to write to {}",
            file_path.as_ref().to_string_lossy()
        )
    })?;

    Ok(())
}

/// Export executable shell script.
pub fn export_script(file_path: impl AsRef<Path>, contents: impl AsRef<str>) -> anyhow::Result<()> {
    export_file(&file_path, contents)?;

    std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o774))
        .context("failed to set file permissions")?;

    Ok(())
}

/// Export an object as JSON.
pub fn export_json(file_path: impl AsRef<Path>, value: impl Serialize) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(&value).context("failed to serialize to JSON")?;

    export_file(file_path, json)
}

/// Read a JSON file, if it exists.
pub fn import_json<T: DeserializeOwned>(file_path: impl AsRef<Path>) -> anyhow::Result<Option<T>> {
    let file_path = file_path.as_ref();
    if file_path.exists() {
        let json = std::fs::read_to_string(file_path)
            .with_context(|| format!("failed to read {}", file_path.to_string_lossy()))?;

        let value = serde_json::from_str::<T>(&json)
            .with_context(|| format!("failed to parse {}", file_path.to_string_lossy()))?;

        Ok(Some(value))
    } else {
        Ok(None)
    }
}
