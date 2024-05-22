// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use fvm_shared::address::Network;
use num_traits::cast::FromPrimitive;

mod commands;

pub use commands::*;
use ipc_provider::config::Config;

/// The trait that represents the abstraction of a command line handler. To implement a new command
/// line operation, implement this trait and register it.
///
/// Note that this trait does not support a stateful implementation as we assume CLI commands are all
/// constructed from scratch.
#[async_trait]
pub trait CommandLineHandler {
    /// Abstraction for command line operations arguments.
    ///
    /// NOTE that this parameter is used to generate the command line arguments.
    /// Currently we are directly integrating with `clap` crate. In the future we can use our own
    /// implementation to abstract away external crates. But this should be good for now.
    type Arguments: std::fmt::Debug + Args;

    /// Handles the request with the provided arguments. Dev should handle the content to print and how
    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()>;
}

/// The global arguments that will be shared by all cli commands.
#[derive(Debug, Args, Clone, Default)]
pub struct GlobalArguments {
    #[arg(
        long,
        help = "The toml config file path for IPC Agent, default to ${HOME}/.ipc/config.toml",
        env = "IPC_CLI_CONFIG_PATH"
    )]
    config_path: Option<String>,

    /// Set the FVM Address Network. It's value affects whether `f` (main) or `t` (test) prefixed addresses are accepted.
    #[arg(long = "network", default_value = "testnet", env = "IPC_NETWORK", value_parser = parse_network)]
    _network: Network,

    /// Legacy env var for network
    #[arg(long = "__network", hide = true, env = "NETWORK", value_parser = parse_network)]
    __network: Option<Network>,
}

impl GlobalArguments {
    pub fn config_path(&self) -> String {
        self.config_path
            .clone()
            .unwrap_or_else(ipc_provider::default_config_path)
    }

    pub fn config(&self) -> Result<Config> {
        let config_path = self.config_path();
        Config::from_file(config_path)
    }

    pub fn network(&self) -> Network {
        self.__network.unwrap_or(self._network)
    }
}

/// Parse the FVM network and set the global value.
fn parse_network(s: &str) -> Result<Network, String> {
    match s.to_lowercase().as_str() {
        "main" | "mainnet" | "f" => Ok(Network::Mainnet),
        "test" | "testnet" | "t" => Ok(Network::Testnet),
        n => {
            let n: u8 = n
                .parse()
                .map_err(|e| format!("expected 0 or 1 for network: {e}"))?;

            let n = Network::from_u8(n).ok_or_else(|| format!("unexpected network: {s}"))?;

            Ok(n)
        }
    }
}
