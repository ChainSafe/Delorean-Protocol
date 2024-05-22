// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::parse::parse_eth_address;
use clap::{Args, Subcommand};
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;

#[derive(Args, Debug)]
pub struct DebugArgs {
    #[command(subcommand)]
    pub command: DebugCommands,
}

#[derive(Subcommand, Debug)]
pub enum DebugCommands {
    /// IPC commands.
    Ipc {
        #[command(subcommand)]
        command: DebugIpcCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum DebugIpcCommands {
    /// Fetch topdown events from the parent and export them to JSON.
    ///
    /// This can be used to construct an upgrade to impute missing events.
    ExportTopDownEvents(Box<DebugExportTopDownEventsArgs>),
}

#[derive(Args, Debug, Clone)]
pub struct DebugExportTopDownEventsArgs {
    /// Child subnet for with the events will be fetched
    #[arg(long, short)]
    pub subnet_id: SubnetID,

    /// Endpoint to the RPC of the child subnet's parent
    #[arg(long, short)]
    pub parent_endpoint: url::Url,

    /// HTTP basic authentication token.
    #[arg(long)]
    pub parent_auth_token: Option<String>,

    /// IPC gateway of the parent; 20 byte Ethereum address in 0x prefixed hex format
    #[arg(long, value_parser = parse_eth_address)]
    pub parent_gateway: Address,

    /// IPC registry of the parent; 20 byte Ethereum address in 0x prefixed hex format
    #[arg(long, value_parser = parse_eth_address)]
    pub parent_registry: Address,

    /// The first block to query for events.
    #[arg(long)]
    pub start_block_height: u64,

    /// The last block to query for events.
    #[arg(long)]
    pub end_block_height: u64,

    /// Location of the JSON file to write events to.
    #[arg(long)]
    pub events_file: PathBuf,
}
