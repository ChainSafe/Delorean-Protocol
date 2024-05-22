// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::econ::TokenAmount;
use serde::Deserialize;
use serde_with::serde_as;

use crate::IsHumanReadable;

#[serde_as]
#[derive(Debug, Deserialize, Clone)]
pub struct FvmSettings {
    /// Overestimation rate applied to gas estimations to ensure that the
    /// message goes through
    pub gas_overestimation_rate: f64,
    /// Gas search step increase used to find the optimal gas limit.
    /// It determines how fine-grained we want the gas estimation to be.
    pub gas_search_step: f64,
    /// Indicate whether transactions should be fully executed during the checks performed
    /// when they are added to the mempool, or just the most basic ones are performed.
    ///
    /// Enabling this option is required to fully support "pending" queries in the Ethereum API,
    /// otherwise only the nonces and balances are projected into a partial state.
    pub exec_in_check: bool,

    /// Gas fee used when broadcasting transactions.
    #[serde_as(as = "IsHumanReadable")]
    pub gas_fee_cap: TokenAmount,
    /// Gas premium used when broadcasting transactions.
    #[serde_as(as = "IsHumanReadable")]
    pub gas_premium: TokenAmount,
}
