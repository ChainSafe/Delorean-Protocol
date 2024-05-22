// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context};
use fendermint_app_options::debug::{
    DebugArgs, DebugCommands, DebugExportTopDownEventsArgs, DebugIpcCommands,
};
use fendermint_vm_topdown::proxy::IPCProviderProxy;
use ipc_provider::{
    config::subnet::{EVMSubnet, SubnetConfig},
    IpcProvider,
};

use crate::cmd;

cmd! {
  DebugArgs(self) {
    match &self.command {
        DebugCommands::Ipc { command } => command.exec(()).await,
    }
  }
}

cmd! {
  DebugIpcCommands(self) {
    match self {
        DebugIpcCommands::ExportTopDownEvents(args) =>
            export_topdown_events(args).await
    }
  }
}

async fn export_topdown_events(args: &DebugExportTopDownEventsArgs) -> anyhow::Result<()> {
    // Configuration for the child subnet on the parent network,
    // based on how it's done in `run.rs` and the `genesis ipc from-parent` command.
    let parent_provider = IpcProvider::new_with_subnet(
        None,
        ipc_provider::config::Subnet {
            id: args
                .subnet_id
                .parent()
                .ok_or_else(|| anyhow!("subnet is not a child"))?,
            config: SubnetConfig::Fevm(EVMSubnet {
                provider_http: args.parent_endpoint.clone(),
                provider_timeout: None,
                auth_token: args.parent_auth_token.clone(),
                registry_addr: args.parent_registry,
                gateway_addr: args.parent_gateway,
            }),
        },
    )?;

    let parent_proxy = IPCProviderProxy::new(parent_provider, args.subnet_id.clone())
        .context("failed to create provider proxy")?;

    let events = fendermint_vm_topdown::sync::fetch_topdown_events(
        &parent_proxy,
        args.start_block_height,
        args.end_block_height,
    )
    .await
    .context("failed to fetch topdown events")?;

    let json = serde_json::to_string_pretty(&events)?;
    std::fs::write(&args.events_file, json)?;

    Ok(())
}
