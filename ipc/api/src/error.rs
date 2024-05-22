// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid subnet id {0}: {1}")]
    InvalidID(String, String),
    #[error("invalid IPC address")]
    InvalidIPCAddr,
    #[error("fvm shared address error")]
    FVMAddressError(fvm_shared::address::Error),

    #[cfg(feature = "fil-actor")]
    #[error("actor error")]
    Actor(fil_actors_runtime::ActorError),
}

#[cfg(feature = "fil-actor")]
impl From<fil_actors_runtime::ActorError> for Error {
    fn from(e: fil_actors_runtime::ActorError) -> Self {
        Self::Actor(e)
    }
}

impl From<fvm_shared::address::Error> for Error {
    fn from(e: fvm_shared::address::Error) -> Self {
        Error::FVMAddressError(e)
    }
}
