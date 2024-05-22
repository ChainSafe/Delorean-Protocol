// Copyright 2022-2024 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

/// Database error
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] rocksdb::Error),
    #[error("{0}")]
    Other(String),
}
