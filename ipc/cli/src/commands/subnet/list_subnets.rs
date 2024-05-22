// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! List subnets cli command

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::fmt::Debug;
use std::str::FromStr;

use crate::{get_ipc_provider, require_fil_addr_from_str, CommandLineHandler, GlobalArguments};

/// The command to create a new subnet actor.
pub(crate) struct ListSubnets;

#[async_trait]
impl CommandLineHandler for ListSubnets {
    type Arguments = ListSubnetsArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("list subnets with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.parent)?;

        let gateway_addr = match &arguments.gateway_address {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        let ls = provider.list_child_subnets(gateway_addr, &subnet).await?;

        for (_, s) in ls.iter() {
            println!(
                "{} - collateral: {} FIL, circ.supply: {} FIL, genesis: {}",
                s.id, s.stake, s.circ_supply, s.genesis_epoch
            );
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(
    name = "list",
    about = "List all child subnets registered in the gateway (i.e. that have provided enough collateral)"
)]
pub(crate) struct ListSubnetsArgs {
    #[arg(long, help = "The gateway address to query subnets")]
    pub gateway_address: Option<String>,
    #[arg(long, help = "The network id to query child subnets")]
    pub parent: String,
}
