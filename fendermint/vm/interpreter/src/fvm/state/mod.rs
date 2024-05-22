// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

mod check;
mod exec;
pub mod fevm;
mod genesis;
pub mod ipc;
mod query;
pub mod snapshot;

use std::sync::Arc;

pub use check::FvmCheckState;
pub use exec::{BlockHash, FvmExecState, FvmStateParams, FvmUpdatableParams};
pub use genesis::{empty_state_tree, FvmGenesisState};
pub use query::FvmQueryState;

use super::store::ReadOnlyBlockstore;

/// We use full state even for checking, to support certain client scenarios.
pub type CheckStateRef<DB> = Arc<tokio::sync::Mutex<Option<FvmExecState<ReadOnlyBlockstore<DB>>>>>;
