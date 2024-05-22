// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Leave subnet cli command handler.

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::{fmt::Debug, str::FromStr};

use crate::{get_ipc_provider, require_fil_addr_from_str, CommandLineHandler, GlobalArguments};

/// The command to leave a new subnet.
pub struct LeaveSubnet;

#[async_trait]
impl CommandLineHandler for LeaveSubnet {
    type Arguments = LeaveSubnetArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("leave subnet with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };
        provider.leave_subnet(subnet, from).await
    }
}

#[derive(Debug, Args)]
#[command(name = "leave", about = "Leaving a subnet")]
pub struct LeaveSubnetArgs {
    #[arg(long, help = "The address that leaves the subnet")]
    pub from: Option<String>,
    #[arg(long, help = "The subnet to leave")]
    pub subnet: String,
}

/// The command to claim collateral for a validator after leaving
pub struct Claim;

#[async_trait]
impl CommandLineHandler for Claim {
    type Arguments = ClaimArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("leave subnet with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };
        provider.claim_collateral(subnet, from).await
    }
}

#[derive(Debug, Args)]
#[command(
    name = "claim",
    about = "Claim collateral or rewards available for validators and relayers, respectively"
)]
pub struct ClaimArgs {
    #[arg(long, help = "The address that claims the collateral")]
    pub from: Option<String>,
    #[arg(long, help = "The subnet to claim from")]
    pub subnet: String,
    #[arg(
        long,
        help = "Determine if we want to claim rewards instead of collateral"
    )]
    pub rewards: bool,
}
