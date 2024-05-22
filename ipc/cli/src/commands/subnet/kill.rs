// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Kill a subnet cli command handler.

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::{fmt::Debug, str::FromStr};

use crate::{get_ipc_provider, require_fil_addr_from_str, CommandLineHandler, GlobalArguments};

/// The command to kill an existing subnet.
pub struct KillSubnet;

#[async_trait]
impl CommandLineHandler for KillSubnet {
    type Arguments = KillSubnetArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("kill subnet with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        provider.kill_subnet(subnet, from).await
    }
}

#[derive(Debug, Args)]
#[command(name = "kill", about = "Kill an existing subnet")]
pub struct KillSubnetArgs {
    #[arg(long, help = "The address that kills the subnet")]
    pub from: Option<String>,
    #[arg(long, help = "The subnet to kill")]
    pub subnet: String,
}
