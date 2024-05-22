// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Propagate cli command handler.

use async_trait::async_trait;
use clap::Args;
use std::fmt::Debug;

use crate::{CommandLineHandler, GlobalArguments};

/// The command to propagate a message in the postbox.
pub(crate) struct Propagate;

#[async_trait]
impl CommandLineHandler for Propagate {
    type Arguments = PropagateArgs;

    async fn handle(_global: &GlobalArguments, _arguments: &Self::Arguments) -> anyhow::Result<()> {
        todo!()
    }
}

#[derive(Debug, Args)]
#[command(about = "Propagate operation in the gateway actor")]
pub(crate) struct PropagateArgs {
    #[arg(long, help = "The JSON RPC server url for ipc agent")]
    pub ipc_agent_url: Option<String>,
    #[arg(long, help = "The address that pays for the propagation gas")]
    pub from: Option<String>,
    #[arg(long, help = "The subnet of the message to propagate")]
    pub subnet: String,
    #[arg(help = "The message cid to propagate")]
    pub postbox_msg_key: String,
}
