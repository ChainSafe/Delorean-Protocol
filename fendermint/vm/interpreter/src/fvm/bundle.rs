// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};
use std::str::FromStr;

fn workspace_dir() -> PathBuf {
    let output = std::process::Command::new(env!("CARGO"))
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .output()
        .unwrap()
        .stdout;
    let cargo_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
    cargo_path.parent().unwrap().to_path_buf()
}

/// Path to the builtin-actor bundle, indended to be used in tests.
pub fn bundle_path() -> PathBuf {
    let bundle_path = std::env::var("FM_BUILTIN_ACTORS_BUNDLE").unwrap_or_else(|_| {
        workspace_dir()
            .join("fendermint/builtin-actors/output/bundle.car")
            .to_string_lossy()
            .into_owned()
    });

    PathBuf::from_str(&bundle_path).expect("malformed bundle path")
}

/// Path to the in-repo custom actor bundle, intended to be used in tests.
pub fn custom_actors_bundle_path() -> PathBuf {
    let custom_actors_bundle_path = std::env::var("FM_CUSTOM_ACTORS_BUNDLE").unwrap_or_else(|_| {
        workspace_dir()
            .join("fendermint/actors/output/custom_actors_bundle.car")
            .to_string_lossy()
            .into_owned()
    });

    PathBuf::from_str(&custom_actors_bundle_path).expect("malformed custom actors bundle path")
}

/// Path to the Solidity contracts, intended to be used in tests.
pub fn contracts_path() -> PathBuf {
    let contracts_path = std::env::var("FM_CONTRACTS_DIR").unwrap_or_else(|_| {
        workspace_dir()
            .join("contracts/out")
            .to_string_lossy()
            .into_owned()
    });

    PathBuf::from_str(&contracts_path).expect("malformed contracts path")
}
