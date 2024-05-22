// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::fmt::Debug;
use std::str::from_utf8;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

pub(crate) struct ShowGatewayContractCommitSha;

#[async_trait]
impl CommandLineHandler for ShowGatewayContractCommitSha {
    type Arguments = ShowGatewayContractCommitShaArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("show contract commit sha with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.network)?;

        let commit_sha = provider.get_commit_sha(&subnet).await?;
        let commit_sha_str = from_utf8(&commit_sha).unwrap();

        println!(
            "Using commit SHA {} for contracts in subnet {}",
            commit_sha_str, subnet
        );

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(
    name = "show-gateway-contract-commit-sha",
    about = "Show code commit SHA for contracts deployed in this network"
)]
pub(crate) struct ShowGatewayContractCommitShaArgs {
    #[arg(long, help = "The network id to query child subnets")]
    pub network: String,
}
