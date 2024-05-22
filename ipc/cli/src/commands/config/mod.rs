// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! This mod triggers a config reload in the IPC-Agent Json RPC server.

mod init;

use clap::{Args, Subcommand};
use std::fmt::Debug;

use crate::commands::config::init::{InitConfig, InitConfigArgs};
use crate::{CommandLineHandler, GlobalArguments};

#[derive(Debug, Args)]
#[command(name = "config", about = "config related commands")]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct ConfigCommandsArgs {
    #[command(subcommand)]
    command: Commands,
}

impl ConfigCommandsArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::Init(args) => InitConfig::handle(global, args).await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    Init(InitConfigArgs),
}
