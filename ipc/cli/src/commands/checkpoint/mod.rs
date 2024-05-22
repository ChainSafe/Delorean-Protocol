// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use crate::commands::checkpoint::bottomup_bundles::{GetBottomUpBundles, GetBottomUpBundlesArgs};
use crate::commands::checkpoint::bottomup_height::{
    LastBottomUpCheckpointHeight, LastBottomUpCheckpointHeightArgs,
};
use crate::commands::checkpoint::list_validator_changes::{
    ListValidatorChanges, ListValidatorChangesArgs,
};
use crate::commands::checkpoint::quorum_reached::{
    GetQuorumReacehdEvents, GetQuorumReachedEventsArgs,
};
use crate::commands::checkpoint::relayer::{BottomUpRelayer, BottomUpRelayerArgs};
use crate::{CommandLineHandler, GlobalArguments};
use clap::{Args, Subcommand};

mod bottomup_bundles;
mod bottomup_height;
mod list_validator_changes;
mod quorum_reached;
mod relayer;

#[derive(Debug, Args)]
#[command(name = "checkpoint", about = "checkpoint related commands")]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CheckpointCommandsArgs {
    #[command(subcommand)]
    command: Commands,
}

impl CheckpointCommandsArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::Relayer(args) => BottomUpRelayer::handle(global, args).await,
            Commands::ListValidatorChanges(args) => {
                ListValidatorChanges::handle(global, args).await
            }
            Commands::ListBottomupBundle(args) => GetBottomUpBundles::handle(global, args).await,
            Commands::QuorumReachedEvents(args) => {
                GetQuorumReacehdEvents::handle(global, args).await
            }
            Commands::LastBottomupCheckpointHeight(args) => {
                LastBottomUpCheckpointHeight::handle(global, args).await
            }
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    Relayer(BottomUpRelayerArgs),
    ListValidatorChanges(ListValidatorChangesArgs),
    ListBottomupBundle(GetBottomUpBundlesArgs),
    QuorumReachedEvents(GetQuorumReachedEventsArgs),
    LastBottomupCheckpointHeight(LastBottomUpCheckpointHeightArgs),
}
