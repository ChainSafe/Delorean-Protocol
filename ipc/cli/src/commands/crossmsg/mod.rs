// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use self::fund::{FundWithToken, FundWithTokenArgs, PreFund, PreFundArgs};
use self::release::{PreRelease, PreReleaseArgs};
use self::topdown_cross::{
    LatestParentFinality, LatestParentFinalityArgs, ListTopdownMsgs, ListTopdownMsgsArgs,
};
use crate::commands::crossmsg::fund::Fund;
use crate::commands::crossmsg::propagate::Propagate;
use crate::commands::crossmsg::release::Release;
use crate::{CommandLineHandler, GlobalArguments};
use fund::FundArgs;
use propagate::PropagateArgs;
use release::ReleaseArgs;

use clap::{Args, Subcommand};

pub mod fund;
pub mod propagate;
pub mod release;
mod topdown_cross;

#[derive(Debug, Args)]
#[command(name = "crossmsg", about = "cross network messages related commands")]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CrossMsgsCommandsArgs {
    #[command(subcommand)]
    command: Commands,
}

impl CrossMsgsCommandsArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::Fund(args) => Fund::handle(global, args).await,
            Commands::FundWithToken(args) => FundWithToken::handle(global, args).await,
            Commands::PreFund(args) => PreFund::handle(global, args).await,
            Commands::Release(args) => Release::handle(global, args).await,
            Commands::PreRelease(args) => PreRelease::handle(global, args).await,
            Commands::Propagate(args) => Propagate::handle(global, args).await,
            Commands::ListTopdownMsgs(args) => ListTopdownMsgs::handle(global, args).await,
            Commands::ParentFinality(args) => LatestParentFinality::handle(global, args).await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    Fund(FundArgs),
    FundWithToken(FundWithTokenArgs),
    PreFund(PreFundArgs),
    Release(ReleaseArgs),
    PreRelease(PreReleaseArgs),
    Propagate(PropagateArgs),
    ListTopdownMsgs(ListTopdownMsgsArgs),
    ParentFinality(LatestParentFinalityArgs),
}
