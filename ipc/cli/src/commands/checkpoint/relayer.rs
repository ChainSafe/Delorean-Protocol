// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use crate::commands::get_subnet_config;
use crate::{require_fil_addr_from_str, CommandLineHandler, GlobalArguments};
use anyhow::anyhow;
use async_trait::async_trait;
use clap::Args;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use ipc_api::subnet_id::SubnetID;
use ipc_provider::checkpoint::BottomUpCheckpointManager;
use ipc_provider::config::Config;
use ipc_provider::new_evm_keystore_from_config;
use ipc_wallet::EvmKeyStore;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

const DEFAULT_POLLING_INTERVAL: u64 = 15;

/// The command to run the bottom up relayer in the background.
pub(crate) struct BottomUpRelayer;

#[async_trait]
impl CommandLineHandler for BottomUpRelayer {
    type Arguments = BottomUpRelayerArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("start bottom up relayer with args: {:?}", arguments);

        let config_path = global.config_path();
        let config = Arc::new(Config::from_file(&config_path)?);
        let mut keystore = new_evm_keystore_from_config(config)?;
        let submitter = match (arguments.submitter.as_ref(), keystore.get_default()?) {
            (Some(submitter), _) => require_fil_addr_from_str(submitter)?,
            (None, Some(addr)) => {
                log::info!("using default address: {addr:?}");
                Address::try_from(addr)?
            }
            _ => {
                return Err(anyhow!("no submitter address provided"));
            }
        };

        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let parent = subnet
            .parent()
            .ok_or_else(|| anyhow!("root does not have parent"))?;

        let child = get_subnet_config(&config_path, &subnet)?;
        let parent = get_subnet_config(&config_path, &parent)?;

        let mut manager = BottomUpCheckpointManager::new_evm_manager(
            parent.clone(),
            child.clone(),
            Arc::new(RwLock::new(keystore)),
            arguments.max_parallelism,
        )
        .await?;

        if let Some(v) = arguments.finalization_blocks {
            manager = manager.with_finalization_blocks(v as ChainEpoch);
        }

        let interval = Duration::from_secs(
            arguments
                .checkpoint_interval_sec
                .unwrap_or(DEFAULT_POLLING_INTERVAL),
        );
        manager.run(submitter, interval).await;

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Start the bottom up relayer daemon")]
pub(crate) struct BottomUpRelayerArgs {
    #[arg(long, help = "The subnet id of the checkpointing subnet")]
    pub subnet: String,
    #[arg(long, help = "The number of seconds to submit checkpoint")]
    pub checkpoint_interval_sec: Option<u64>,
    #[arg(
        long,
        default_value = "0",
        help = "The number of blocks away from chain head that is considered final"
    )]
    pub finalization_blocks: Option<u64>,
    #[arg(long, help = "The hex encoded address of the submitter")]
    pub submitter: Option<String>,
    #[arg(
        long,
        default_value = "4",
        help = "The max parallelism for submitting checkpoints"
    )]
    pub max_parallelism: usize,
}
