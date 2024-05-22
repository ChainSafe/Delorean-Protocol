// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{BlockHeight, SequentialAppendError};
use thiserror::Error;

/// The errors for top down checkpointing
#[derive(Error, Debug, Eq, PartialEq, Clone)]
pub enum Error {
    #[error("Incoming items are not order sequentially")]
    NotSequential,
    #[error("The parent view update with block height is not sequential: {0:?}")]
    NonSequentialParentViewInsert(SequentialAppendError),
    #[error("Parent chain reorg detected")]
    ParentChainReorgDetected,
    #[error("Cannot query parent at height {1}: {0}")]
    CannotQueryParent(String, BlockHeight),
}
