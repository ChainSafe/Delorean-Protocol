// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! List validator change set cli command

use std::fmt::Debug;
use std::str::FromStr;

use async_trait::async_trait;
use clap::Args;
use fvm_shared::clock::ChainEpoch;
use ipc_api::subnet_id::SubnetID;

use crate::commands::get_ipc_provider;
use crate::{CommandLineHandler, GlobalArguments};

/// The command to list validator changes committed in a subnet.
pub(crate) struct ListValidatorChanges;

#[async_trait]
impl CommandLineHandler for ListValidatorChanges {
    type Arguments = ListValidatorChangesArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("list validator changes with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        for h in arguments.from_epoch..=arguments.to_epoch {
            let changes = provider.get_validator_changeset(&subnet, h).await?;
            log::info!("changes at height: {h} are: {:?}", changes.value);
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "List of validator changes commmitted for a child subnet")]
pub(crate) struct ListValidatorChangesArgs {
    #[arg(long, help = "Lists the validator changes between two epochs")]
    pub subnet: String,
    #[arg(long, help = "Include checkpoints from this epoch")]
    pub from_epoch: ChainEpoch,
    #[arg(long, help = "Include checkpoints up to this epoch")]
    pub to_epoch: ChainEpoch,
}
