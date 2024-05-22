// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use tendermint_rpc::{Url, WebSocketClientUrl};

#[derive(Args, Debug)]
pub struct EthArgs {
    #[command(subcommand)]
    pub command: EthCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum EthCommands {
    /// Run the Ethereum JSON-RPC facade.
    Run {
        /// The URL of the Tendermint node's RPC endpoint.
        #[arg(
            long,
            short,
            default_value = "http://127.0.0.1:26657",
            env = "TENDERMINT_RPC_URL"
        )]
        http_url: Url,

        /// The URL of the Tendermint node's WebSocket endpoint.
        #[arg(
            long,
            short,
            default_value = "ws://127.0.0.1:26657/websocket",
            env = "TENDERMINT_WS_URL"
        )]
        ws_url: WebSocketClientUrl,

        /// Seconds to wait between trying to connect to the websocket.
        #[arg(long, short = 'd', default_value = "5")]
        connect_retry_delay: u64,
    },
}
