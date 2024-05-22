// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Subnet bootstrap-related commands

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::{fmt::Debug, str::FromStr};

use crate::{get_ipc_provider, require_fil_addr_from_str, CommandLineHandler, GlobalArguments};

/// The command to add a bootstrap subnet
pub struct AddBootstrap;

#[async_trait]
impl CommandLineHandler for AddBootstrap {
    type Arguments = AddBootstrapArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("add subnet bootstrap with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        provider
            .add_bootstrap(&subnet, from, arguments.endpoint.clone())
            .await
    }
}

#[derive(Debug, Args)]
#[command(name = "add-bootstrap", about = "Advertise bootstrap in the subnet")]
pub struct AddBootstrapArgs {
    #[arg(long, help = "The address of the validator adding the bootstrap")]
    pub from: Option<String>,
    #[arg(long, help = "The subnet to add the bootstrap to")]
    pub subnet: String,
    #[arg(long, help = "The bootstrap node's network endpoint")]
    pub endpoint: String,
}

/// The command to list bootstrap nodes in a subnet
pub struct ListBootstraps;

#[async_trait]
impl CommandLineHandler for ListBootstraps {
    type Arguments = ListBootstrapsArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("add subnet bootstrap with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        print!(
            "{}",
            provider
                .list_bootstrap_nodes(&subnet)
                .await?
                .iter()
                .as_slice()
                .join(",")
        );

        println!();
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(name = "list-bootstraps", about = "List bootstraps in the subnet")]
pub struct ListBootstrapsArgs {
    #[arg(long, help = "The subnet to list bootstraps from")]
    pub subnet: String,
}
