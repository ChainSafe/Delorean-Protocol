// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! List top down cross messages

use std::fmt::Debug;
use std::str::FromStr;

use async_trait::async_trait;
use clap::Args;
use fvm_shared::clock::ChainEpoch;
use ipc_api::subnet_id::SubnetID;

use crate::commands::get_ipc_provider;
use crate::{CommandLineHandler, GlobalArguments};

/// The command to list top down cross messages in a subnet
pub(crate) struct ListTopdownMsgs;

#[async_trait]
impl CommandLineHandler for ListTopdownMsgs {
    type Arguments = ListTopdownMsgsArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("list topdown messages with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        for h in arguments.from..=arguments.to {
            let result = provider.get_top_down_msgs(&subnet, h).await?;
            println!(
                "block height: {}, block hash: {}, number of messages: {}",
                h,
                hex::encode(result.block_hash),
                result.value.len()
            );
            for msg in result.value {
                println!(
                    "from: {}, to: {}, message: {}, nonce: {} ",
                    msg.from.to_string()?,
                    msg.to.to_string()?,
                    hex::encode(msg.message),
                    msg.nonce
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "List topdown cross messages for a specific epoch")]
pub(crate) struct ListTopdownMsgsArgs {
    #[arg(long, help = "The subnet id of the topdown subnet")]
    pub subnet: String,
    #[arg(long, help = "Include topdown messages starting from this epoch")]
    pub from: ChainEpoch,
    #[arg(long, help = "Include topdown messages to this epoch")]
    pub to: ChainEpoch,
}

pub(crate) struct LatestParentFinality;

#[async_trait]
impl CommandLineHandler for LatestParentFinality {
    type Arguments = LatestParentFinalityArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("latest parent finality: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        println!("{}", provider.latest_parent_finality(&subnet).await?);
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Latest height of parent finality committed in child subnet")]
pub(crate) struct LatestParentFinalityArgs {
    #[arg(long, help = "The subnet id to check parent finality")]
    pub subnet: String,
}
