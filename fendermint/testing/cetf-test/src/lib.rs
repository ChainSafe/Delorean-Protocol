// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Run tests against multiple Fendermint+CometBFT docker container pairs locally:
//! 0. The default `snapshot-fendermint` and `snapshot-cometbft` pair
//! 1. A `snapshot-cometbft-1` and `snapshot-cometbft-2`, using `scripts/node-1.env` and `node-2`.env,
//!    syncing with the default node from genesis on a block-by-block basis, and clear out their history
//!    to force others who sync with them to use snapshots.
//! 2. A `snapshot-cometbft-3` using `scripts/node-3.env`,
//!    which syncs with `node-1` and `node-2` using snapshots (a.k.a. state sync).
//!
//! Note that CometBFT state sync requires 2 RPC servers, which is why we need 3 nodes.
//!
//! See <https://docs.cometbft.com/v0.37/core/state-sync> and <https://docs.cometbft.com/v0.37/core/configuration>
//!
//! Examples:
//!
//! 1. All in one go
//! ```text
//! cd fendermint/testing/snapshot-test
//! cargo make
//! ```
//!
//! 2. One by one
//! ```text
//! cd fendermint/testing/snapshot-test
//! cargo make setup
//! cargo make node-1-setup
//! cargo make node-2-setup
//! cargo make node-3-setup
//! docker logs snapshot-cometbft-3
//! cargo make snapshot-teardown
//! cargo make teardown
//! ```
//!
//! Make sure you installed cargo-make by running `cargo install cargo-make` first.
