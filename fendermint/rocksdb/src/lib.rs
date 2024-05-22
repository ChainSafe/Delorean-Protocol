// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
mod rocks;

#[cfg(feature = "blockstore")]
pub mod blockstore;
#[cfg(feature = "kvstore")]
mod kvstore;

pub mod namespaces;

pub use rocks::{Error as RocksDbError, RocksDb, RocksDbConfig};
