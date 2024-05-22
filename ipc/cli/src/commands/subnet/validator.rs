// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Get the validator information

use async_trait::async_trait;
use clap::Args;
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;
use std::fmt::Debug;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

/// The command to get the validator information
pub(crate) struct ValidatorInfo;

#[async_trait]
impl CommandLineHandler for ValidatorInfo {
    type Arguments = ValidatorInfoArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("get validator info with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let validator = Address::from_str(&arguments.validator)?;

        let validator_info = provider.get_validator_info(&subnet, &validator).await?;
        println!("{}", validator_info);

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(name = "validator-info", about = "Get the validator info")]
pub(crate) struct ValidatorInfoArgs {
    #[arg(long, help = "The subnet id to query validator info")]
    pub subnet: String,
    #[arg(long, help = "The validator address")]
    pub validator: String,
}
