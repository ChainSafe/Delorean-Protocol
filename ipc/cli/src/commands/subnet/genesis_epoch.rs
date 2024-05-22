// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Get the genesis epoch cli command

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::fmt::Debug;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

/// The command to get the genensis epoch.
pub(crate) struct GenesisEpoch;

#[async_trait]
impl CommandLineHandler for GenesisEpoch {
    type Arguments = GenesisEpochArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("get genesis epoch with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        let ls = provider.genesis_epoch(&subnet).await?;
        println!("genesis epoch: {}", ls);

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(name = "genesis-epoch", about = "Get the genesis epoch of subnet")]
pub(crate) struct GenesisEpochArgs {
    #[arg(long, help = "The subnet id to query genesis epoch")]
    pub subnet: String,
}
