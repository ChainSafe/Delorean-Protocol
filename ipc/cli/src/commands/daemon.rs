// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! The Daemon command line handler that prints the info about IPC Agent.

use std::fmt::Debug;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use clap::Args;
use ipc_wallet::Wallet;
use tokio_graceful_shutdown::{IntoSubsystem, Toplevel};

use crate::checkpoint::CheckpointSubsystem;
use crate::config::ReloadableConfig;
use crate::server::jsonrpc::JsonRPCServer;
use crate::server::{new_evm_keystore_from_config, new_fvm_wallet_from_config};
use crate::{CommandLineHandler, GlobalArguments};

/// The number of seconds to wait for a subsystem to start before returning an error.
const SUBSYSTEM_WAIT_TIME_SECS: Duration = Duration::from_secs(10);

/// The command to start the ipc agent json rpc server in the foreground.
pub(crate) struct LaunchDaemon;

#[async_trait]
impl CommandLineHandler for LaunchDaemon {
    type Arguments = LaunchDaemonArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!(
            "launching json rpc server with args: {:?} and global params: {:?}",
            arguments,
            global
        );

        let reloadable_config = Arc::new(ReloadableConfig::new(global.config_path())?);
        let fvm_wallet = Arc::new(RwLock::new(Wallet::new(new_fvm_wallet_from_config(
            reloadable_config.clone(),
        )?)));
        let evm_keystore = Arc::new(RwLock::new(new_evm_keystore_from_config(
            reloadable_config.clone(),
        )?));

        // Start subsystems.
        let checkpointing = CheckpointSubsystem::new(
            reloadable_config.clone(),
            fvm_wallet.clone(),
            evm_keystore.clone(),
        );
        let server = JsonRPCServer::new(
            reloadable_config.clone(),
            fvm_wallet.clone(),
            evm_keystore.clone(),
        );
        Toplevel::new()
            .start("Checkpoint subsystem", checkpointing.into_subsystem())
            .start("JSON-RPC server subsystem", server.into_subsystem())
            .catch_signals()
            .handle_shutdown_requests(SUBSYSTEM_WAIT_TIME_SECS)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Launch the ipc agent daemon process")]
pub(crate) struct LaunchDaemonArgs {}
