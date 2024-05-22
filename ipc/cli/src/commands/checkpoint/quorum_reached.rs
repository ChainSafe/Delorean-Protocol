// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! List quorum reached events

use std::fmt::Debug;
use std::str::FromStr;

use async_trait::async_trait;
use clap::Args;
use fvm_shared::clock::ChainEpoch;
use ipc_api::subnet_id::SubnetID;

use crate::commands::get_ipc_provider;
use crate::{CommandLineHandler, GlobalArguments};

/// The command to list quorum reached at height.
pub(crate) struct GetQuorumReacehdEvents;

#[async_trait]
impl CommandLineHandler for GetQuorumReacehdEvents {
    type Arguments = GetQuorumReachedEventsArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("get quorum reached events with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        for h in arguments.from_epoch..=arguments.to_epoch {
            let events = provider.quorum_reached_events(&subnet, h).await?;
            for e in events {
                println!("{e}");
            }
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "List quorum reached events for a child subnet")]
pub(crate) struct GetQuorumReachedEventsArgs {
    #[arg(long, help = "The target subnet to perform query")]
    pub subnet: String,
    #[arg(long, help = "Include events from this epoch")]
    pub from_epoch: ChainEpoch,
    #[arg(long, help = "Include events up to this epoch")]
    pub to_epoch: ChainEpoch,
}
