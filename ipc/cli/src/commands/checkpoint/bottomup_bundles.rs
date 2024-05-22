// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! List bottom up bundles

use std::fmt::Debug;
use std::str::FromStr;

use async_trait::async_trait;
use clap::Args;
use fvm_shared::clock::ChainEpoch;
use ipc_api::subnet_id::SubnetID;

use crate::commands::get_ipc_provider;
use crate::{CommandLineHandler, GlobalArguments};

/// The command to get bottom up bundles at height.
pub(crate) struct GetBottomUpBundles;

#[async_trait]
impl CommandLineHandler for GetBottomUpBundles {
    type Arguments = GetBottomUpBundlesArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("get bottom up bundles with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        for h in arguments.from_epoch..=arguments.to_epoch {
            let Some(bundle) = provider.get_bottom_up_bundle(&subnet, h).await? else {
                continue;
            };

            println!("bottom up checkpoint bundle at height: {}", h);
            println!("{}", serde_json::to_string(&bundle)?);
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "List bottom up checkpoint signature bundle for a child subnet")]
pub(crate) struct GetBottomUpBundlesArgs {
    #[arg(long, help = "The target subnet to perform query")]
    pub subnet: String,
    #[arg(long, help = "Include checkpoints from this epoch")]
    pub from_epoch: ChainEpoch,
    #[arg(long, help = "Include checkpoints up to this epoch")]
    pub to_epoch: ChainEpoch,
}
