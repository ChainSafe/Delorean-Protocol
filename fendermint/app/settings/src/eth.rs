// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::econ::TokenAmount;
use serde::Deserialize;
use serde_with::{serde_as, DurationSeconds};
use std::time::Duration;

use crate::{IsHumanReadable, SocketAddress};

/// Ethereum API facade settings.
#[serde_as]
#[derive(Debug, Deserialize, Clone)]
pub struct EthSettings {
    pub listen: SocketAddress,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub filter_timeout: Duration,
    pub cache_capacity: usize,
    pub gas: GasOpt,
    pub max_nonce_gap: u64,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct GasOpt {
    /// Minimum gas fee in atto.
    #[serde_as(as = "IsHumanReadable")]
    pub min_gas_premium: TokenAmount,
    pub num_blocks_max_prio_fee: u64,
    pub max_fee_hist_size: u64,
}
