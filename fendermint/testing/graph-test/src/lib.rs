// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Run tests against a local Fendermint docker container node:
//! 0. The default `graph-fendermint`, `graph-cometbft` and `graph-ethapi` triplet
//! 1. The Graph docker-compose setup connecting to Fendermint through the Ethereum API
//!
//! Note that CometBFT state sync requires 2 RPC servers, which is why we need 3 nodes.
//!
//! See
//! * <https://github.com/graphprotocol/graph-node/blob/master/docker/README.md>
//! * <https://docs.hedera.com/hedera/tutorials/smart-contracts/deploy-a-subgraph-using-the-graph-and-json-rpc>
//! * <https://github.com/hashgraph/hedera-subgraph-example>
//! * <https://github.com/hashgraph/hedera-hardhat-example-project>
//!
//! Examples:
//!
//! 1. All in one go
//! ```text
//! cd fendermint/testing/graph-test
//! cargo make
//! ```
//!
//! 2. One by one
//! ```text
//! cd fendermint/testing/graph-test
//! cargo make setup
//! cargo make test
//! cargo make teardown
//! ```
//!
//! Make sure you installed cargo-make by running `cargo install cargo-make` first.
