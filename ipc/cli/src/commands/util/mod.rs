// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use crate::{CommandLineHandler, GlobalArguments};

use clap::{Args, Subcommand};

use self::f4::{EthToF4Addr, EthToF4AddrArgs};

mod f4;

#[derive(Debug, Args)]
#[command(name = "util", about = "util commands")]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UtilCommandsArgs {
    #[command(subcommand)]
    command: Commands,
}

impl UtilCommandsArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::EthToF4Addr(args) => EthToF4Addr::handle(global, args).await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    EthToF4Addr(EthToF4AddrArgs),
}
