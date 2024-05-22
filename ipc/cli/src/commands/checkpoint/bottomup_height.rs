// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use std::fmt::Debug;
use std::str::FromStr;

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;

use crate::commands::get_ipc_provider;
use crate::{CommandLineHandler, GlobalArguments};

/// The command to get the last bottom up checkpoint height in a subnet.
pub(crate) struct LastBottomUpCheckpointHeight;

#[async_trait]
impl CommandLineHandler for LastBottomUpCheckpointHeight {
    type Arguments = LastBottomUpCheckpointHeightArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!(
            "list bottom up checkpoint height with args: {:?}",
            arguments
        );

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        let height = provider.last_bottom_up_checkpoint_height(&subnet).await?;
        println!("height: {height}");

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Last bottom up checkpoint height committed for a child subnet in the parent")]
pub(crate) struct LastBottomUpCheckpointHeightArgs {
    #[arg(long, help = "The target subnet to perform query")]
    pub subnet: String,
}
