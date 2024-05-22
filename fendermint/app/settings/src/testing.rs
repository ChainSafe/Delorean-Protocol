// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct TestingSettings {
    /// Indicate whether the chain metadata should be pushed into the ledger.
    ///
    /// Doing so causes the ledger to change even on empty blocks, which will
    /// cause another empty block to be created by CometBFT, perpetuating
    /// it even if we don't want them.
    ///
    /// See <https://docs.cometbft.com/v0.37/core/configuration#empty-blocks-vs-no-empty-blocks>
    ///
    /// This is here for testing purposes only, it should be `true` by default to allow
    /// the `evm` actor to execute the `BLOCKHASH` function.
    pub push_chain_meta: bool,
}
