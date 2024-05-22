// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Utility methods and entry point for tests using the docker materializer.
//!
//! # Example
//!
//! `cargo test -p fendermint_materializer --test docker -- --nocapture`

use std::{
    collections::BTreeSet,
    env::current_dir,
    path::PathBuf,
    pin::Pin,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context};
use ethers::providers::Middleware;
use fendermint_materializer::{
    docker::{DockerMaterializer, DockerMaterials},
    manifest::Manifest,
    testnet::Testnet,
    HasCometBftApi, HasEthApi, TestnetName,
};
use futures::Future;
use lazy_static::lazy_static;
use tendermint_rpc::Client;

pub type DockerTestnet = Testnet<DockerMaterials, DockerMaterializer>;

lazy_static! {
    static ref CI_PROFILE: bool = std::env::var("PROFILE").unwrap_or_default() == "ci";
    static ref STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
    static ref TEARDOWN_TIMEOUT: Duration = Duration::from_secs(30);
    static ref PRINT_LOGS_ON_ERROR: bool = *CI_PROFILE;
}

/// Want to keep the testnet artifacts in the `tests/testnets` directory.
fn tests_dir() -> PathBuf {
    let dir = current_dir().unwrap();
    debug_assert!(
        dir.ends_with("materializer"),
        "expected the current directory to be the crate"
    );
    dir.join("tests")
}

/// Directory where we keep the docker-materializer related data files.
fn test_data_dir() -> PathBuf {
    tests_dir().join("docker-materializer-data")
}

/// Parse a manifest from the `tests/manifests` directory.
fn read_manifest(file_name: &str) -> anyhow::Result<Manifest> {
    let manifest = tests_dir().join("manifests").join(file_name);
    let manifest = Manifest::from_file(&manifest)?;
    Ok(manifest)
}

/// Parse a manifest file in the `manifests` directory, clean up any corresponding
/// testnet resources, then materialize a testnet and run some tests.
pub async fn with_testnet<F, G>(manifest_file_name: &str, alter: G, test: F) -> anyhow::Result<()>
where
    // https://users.rust-lang.org/t/function-that-takes-a-closure-with-mutable-reference-that-returns-a-future/54324
    F: for<'a> FnOnce(
        &Manifest,
        &mut DockerMaterializer,
        &'a mut DockerTestnet,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>>,
    G: FnOnce(&mut Manifest),
{
    let testnet_name = TestnetName::new(
        PathBuf::from(manifest_file_name)
            .file_stem()
            .expect("filename missing")
            .to_string_lossy()
            .to_string(),
    );

    let mut manifest = read_manifest(manifest_file_name)?;

    // Make any test-specific modifications to the manifest if that makes sense.
    alter(&mut manifest);

    // Make sure it's a sound manifest.
    manifest
        .validate(&testnet_name)
        .await
        .context("failed to validate manifest")?;

    // NOTE: Add `with_policy(DropPolicy::PERSISTENT)` if you want containers to stick around for inspection,
    // but logs and env vars should be available on disk even if the testnet is torn down at the end.
    let mut materializer = DockerMaterializer::new(&test_data_dir(), 0)?;

    // make sure we start with clean slate by removing any previous files
    materializer
        .remove(&testnet_name)
        .await
        .context("failed to remove testnet")?;

    let mut testnet = Testnet::setup(&mut materializer, &testnet_name, &manifest)
        .await
        .context("failed to set up testnet")?;

    let started = wait_for_startup(&testnet).await?;

    let res = if started {
        test(&manifest, &mut materializer, &mut testnet).await
    } else {
        Err(anyhow!("the startup sequence timed out"))
    };

    // Print all logs on failure.
    // Some might be available in logs in the files which are left behind,
    // e.g. for `fendermint` we have logs, but maybe not for `cometbft`.
    if res.is_err() && *PRINT_LOGS_ON_ERROR {
        for (name, node) in testnet.nodes() {
            let name = name.path_string();
            for log in node.fendermint_logs().await {
                eprintln!("{name}/fendermint: {log}");
            }
            for log in node.cometbft_logs().await {
                eprintln!("{name}/cometbft: {log}");
            }
            for log in node.ethapi_logs().await {
                eprintln!("{name}/ethapi: {log}");
            }
        }
    }

    // Tear down the testnet.
    drop(testnet);

    // Allow some time for containers to be dropped.
    // This only happens if the testnet setup succeeded,
    // otherwise the system shuts down too quick, but
    // at least we can inspect the containers.
    // If they don't all get dropped, `docker system prune` helps.
    let drop_handle = materializer.take_dropper();
    let _ = tokio::time::timeout(*TEARDOWN_TIMEOUT, drop_handle).await;

    res
}

/// Allow time for things to consolidate and APIs to start.
async fn wait_for_startup(testnet: &DockerTestnet) -> anyhow::Result<bool> {
    let start = Instant::now();
    let mut started = BTreeSet::new();

    'startup: loop {
        if start.elapsed() > *STARTUP_TIMEOUT {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;

        for (name, dnode) in testnet.nodes() {
            if started.contains(name) {
                continue;
            }

            let client = dnode.cometbft_http_provider()?;

            if let Err(e) = client.abci_info().await {
                eprintln!("CometBFT on {name} still fails: {e}");
                continue 'startup;
            }

            if let Some(client) = dnode.ethapi_http_provider()? {
                if let Err(e) = client.get_chainid().await {
                    eprintln!("EthAPI on {name} still fails: {e}");
                    continue 'startup;
                }
            }

            eprintln!("APIs on {name} started");
            started.insert(name.clone());
        }

        // All of them succeeded.
        return Ok(true);
    }
}

// Run these tests serially because they share a common `materializer-state.json` file with the port mappings.
// Unfortunately the `#[serial]` macro can only be applied to module blocks, not this.
mod docker_tests;
