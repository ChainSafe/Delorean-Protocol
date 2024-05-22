// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_vm_interpreter::fvm::state::snapshot::SnapshotVersion;

/// Possible errors with snapshots.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("incompatible snapshot version: {0}")]
    IncompatibleVersion(SnapshotVersion),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("there is no ongoing snapshot download")]
    NoDownload,
    #[error("unexpected chunk index; expected {0}, got {1}")]
    UnexpectedChunk(u32, u32),
    #[error("wrong checksum; expected {0}, got {1}")]
    WrongChecksum(tendermint::Hash, tendermint::Hash),
}
