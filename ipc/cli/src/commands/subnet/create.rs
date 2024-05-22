// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Create subnet cli command handler.

use std::fmt::Debug;
use std::str::FromStr;

use async_trait::async_trait;
use clap::Args;
use fvm_shared::clock::ChainEpoch;

use ipc_api::subnet::{PermissionMode, SupplyKind, SupplySource};
use ipc_api::subnet_id::SubnetID;

use crate::commands::get_ipc_provider;
use crate::{f64_to_token_amount, require_fil_addr_from_str, CommandLineHandler, GlobalArguments};

const DEFAULT_ACTIVE_VALIDATORS: u16 = 100;

/// The command to create a new subnet actor.
pub struct CreateSubnet;

impl CreateSubnet {
    pub async fn create(
        global: &GlobalArguments,
        arguments: &CreateSubnetArgs,
    ) -> anyhow::Result<String> {
        let mut provider = get_ipc_provider(global)?;
        let parent = SubnetID::from_str(&arguments.parent)?;

        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        let token_address = if let Some(addr) = &arguments.supply_source_address {
            Some(require_fil_addr_from_str(addr)?)
        } else {
            None
        };
        let supply_source = SupplySource {
            kind: arguments.supply_source_kind,
            token_address,
        };
        let addr = provider
            .create_subnet(
                from,
                parent,
                arguments.min_validators,
                f64_to_token_amount(arguments.min_validator_stake)?,
                arguments.bottomup_check_period,
                arguments
                    .active_validators_limit
                    .unwrap_or(DEFAULT_ACTIVE_VALIDATORS),
                f64_to_token_amount(arguments.min_cross_msg_fee)?,
                arguments.permission_mode,
                supply_source,
            )
            .await?;

        Ok(addr.to_string())
    }
}

#[async_trait]
impl CommandLineHandler for CreateSubnet {
    type Arguments = CreateSubnetArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("create subnet with args: {:?}", arguments);

        let address = CreateSubnet::create(global, arguments).await?;

        log::info!(
            "created subnet actor with id: {}/{}",
            arguments.parent,
            address
        );

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(name = "create", about = "Create a new subnet actor")]
pub struct CreateSubnetArgs {
    #[arg(long, help = "The address that creates the subnet")]
    pub from: Option<String>,
    #[arg(long, help = "The parent subnet to create the new actor in")]
    pub parent: String,
    #[arg(
        long,
        help = "The minimum number of collateral required for validators in (in whole FIL; the minimum is 1 nanoFIL)"
    )]
    pub min_validator_stake: f64,
    #[arg(
        long,
        help = "Minimum number of validators required to bootstrap the subnet"
    )]
    pub min_validators: u64,
    #[arg(long, help = "The bottom up checkpoint period in number of blocks")]
    pub bottomup_check_period: ChainEpoch,
    #[arg(long, help = "The max number of active validators in subnet")]
    pub active_validators_limit: Option<u16>,
    #[arg(
        long,
        default_value = "0.000001",
        help = "Minimum fee for cross-net messages in subnet (in whole FIL; the minimum is 1 nanoFIL)"
    )]
    pub min_cross_msg_fee: f64,
    #[arg(
        long,
        help = "The permission mode for the subnet: collateral, federated and static",
        value_parser = PermissionMode::from_str,
    )]
    // TODO figure out a way to use a newtype + ValueEnum, or reference PermissionMode::VARIANTS to
    //  enumerate all variants
    pub permission_mode: PermissionMode,
    #[arg(
        long,
        help = "The kind of supply source of a subnet on its parent subnet: native or erc20",
        value_parser = SupplyKind::from_str,
    )]
    // TODO figure out a way to use a newtype + ValueEnum, or reference SupplySourceKind::VARIANTS to
    //  enumerate all variants
    pub supply_source_kind: SupplyKind,
    #[arg(
        long,
        help = "The address of supply source of a subnet on its parent subnet. None if kind is native"
    )]
    pub supply_source_address: Option<String>,
}
