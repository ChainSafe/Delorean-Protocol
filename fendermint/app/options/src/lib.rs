// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use config::ConfigArgs;
use debug::DebugArgs;
use fvm_shared::address::Network;
use lazy_static::lazy_static;
use tracing_subscriber::EnvFilter;

use self::{
    eth::EthArgs, genesis::GenesisArgs, key::KeyArgs, materializer::MaterializerArgs, rpc::RpcArgs,
    run::RunArgs,
};

pub mod config;
pub mod debug;
pub mod eth;
pub mod genesis;
pub mod key;
pub mod materializer;
pub mod rpc;
pub mod run;

mod log;
mod parse;

use log::{parse_log_level, LogLevel};
use parse::parse_network;

lazy_static! {
    static ref ENV_ALIASES: Vec<(&'static str, Vec<&'static str>)> = vec![
        ("FM_NETWORK", vec!["IPC_NETWORK", "NETWORK"]),
        ("FM_LOG_LEVEL", vec!["LOG_LEVEL", "RUST_LOG"])
    ];
}

/// Parse the main arguments by:
/// 0. Detecting aliased env vars
/// 1. Parsing the [GlobalOptions]
/// 2. Setting any system wide parameters based on the globals
/// 3. Parsing and returning the final [Options]
pub fn parse() -> Options {
    set_env_from_aliases();
    let opts: GlobalOptions = GlobalOptions::parse();
    fvm_shared::address::set_current_network(opts.global.network);
    let opts: Options = Options::parse();
    opts
}

/// Assign value to env vars from aliases, if the canonic key doesn't exist but the alias does.
fn set_env_from_aliases() {
    'keys: for (key, aliases) in ENV_ALIASES.iter() {
        for alias in aliases {
            if let (Err(_), Ok(value)) = (std::env::var(key), std::env::var(alias)) {
                std::env::set_var(key, value);
                continue 'keys;
            }
        }
    }
}

#[derive(Args, Debug)]
pub struct GlobalArgs {
    /// Set the FVM Address Network. It's value affects whether `f` (main) or `t` (test) prefixed addresses are accepted.
    #[arg(short, long, default_value = "mainnet", env = "FM_NETWORK", value_parser = parse_network)]
    pub network: Network,
}

/// A version of options that does partial matching on the arguments, with its only interest
/// being the capture of global parameters that need to take effect first, before we parse [Options],
/// because their value affects how others arse parsed.
///
/// This one doesn't handle `--help` or `help` so that it is passed on to the next parser,
/// where the full set of commands and arguments can be printed properly.
#[derive(Parser, Debug)]
#[command(version, disable_help_flag = true)]
pub struct GlobalOptions {
    #[command(flatten)]
    pub global: GlobalArgs,

    /// Capture all the normal commands, basically to ingore them.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    pub cmd: Vec<String>,
}

#[derive(Parser, Debug)]
#[command(version)]
pub struct Options {
    /// Set a custom directory for data and configuration files.
    #[arg(
        short = 'd',
        long,
        default_value = "~/.fendermint",
        env = "FM_HOME_DIR"
    )]
    pub home_dir: PathBuf,

    /// Set a custom directory for configuration files
    #[arg(long, env = "FM_CONFIG_DIR")]
    config_dir: Option<PathBuf>,

    /// Set a custom directory for ipc log files.
    #[arg(long, env = "FM_LOG_DIR")]
    pub log_dir: Option<PathBuf>,

    /// Set a custom prefix for ipc log files.
    #[arg(long, env = "FM_LOG_FILE_PREFIX")]
    pub log_file_prefix: Option<String>,

    /// Optionally override the default configuration.
    #[arg(short, long, default_value = "dev")]
    pub mode: String,

    /// Set the logging level of the console.
    #[arg(
        short = 'l',
        long,
        default_value = "info",
        value_enum,
        env = "FM_LOG_LEVEL",
        help = "Standard log levels, or a comma separated list of filters, e.g. 'debug,tower_abci=warn,libp2p::gossipsub=info'",
        value_parser = parse_log_level,
    )]
    log_level: LogLevel,

    /// Set the logging level of the log file. If missing, it defaults to the same level as the console.
    #[arg(
        long,
        value_enum,
        env = "FM_LOG_FILE_LEVEL",
        value_parser = parse_log_level,
    )]
    log_file_level: Option<LogLevel>,

    /// Global options repeated here for discoverability, so they show up in `--help` among the others.
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Commands,
}

impl Options {
    /// Tracing filter for the console.
    ///
    /// Coalescing everything into a filter instead of either a level or a filter
    /// because the `tracing_subscriber` setup methods like `with_filter` and `with_level`
    /// produce different static types and it's not obvious how to use them as alternatives.
    pub fn log_console_filter(&self) -> anyhow::Result<EnvFilter> {
        self.log_level.to_filter()
    }

    /// Tracing filter for the log file.
    pub fn log_file_filter(&self) -> anyhow::Result<EnvFilter> {
        if let Some(ref level) = self.log_file_level {
            level.to_filter()
        } else {
            self.log_console_filter()
        }
    }

    /// Path to the configuration directories.
    ///
    /// If not specified then returns the default under the home directory.
    pub fn config_dir(&self) -> PathBuf {
        self.config_dir
            .as_ref()
            .cloned()
            .unwrap_or(self.home_dir.join("config"))
    }

    /// Check if metrics are supposed to be collected.
    pub fn metrics_enabled(&self) -> bool {
        matches!(self.command, Commands::Run(_) | Commands::Eth(_))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Parse the configuration file and print it to the console.
    Config(ConfigArgs),
    /// Arbitrary commands that aid in debugging.
    Debug(DebugArgs),
    /// Run the `App`, listening to ABCI requests from Tendermint.
    Run(RunArgs),
    /// Subcommands related to the construction of signing keys.
    Key(KeyArgs),
    /// Subcommands related to the construction of Genesis files.
    Genesis(GenesisArgs),
    /// Subcommands related to sending JSON-RPC commands/queries to Tendermint.
    Rpc(RpcArgs),
    /// Subcommands related to the Ethereum API facade.
    Eth(EthArgs),
    /// Subcommands related to the Testnet Materializer.
    #[clap(aliases  = &["mat", "matr", "mate"])]
    Materializer(MaterializerArgs),
}

#[cfg(test)]
mod tests {
    use crate::*;
    use clap::Parser;
    use fvm_shared::address::Network;
    use tracing::level_filters::LevelFilter;

    /// Set some env vars, run a fallible piece of code, then unset the variables otherwise they would affect the next test.
    pub fn with_env_vars<F, T>(vars: &[(&str, &str)], f: F) -> T
    where
        F: FnOnce() -> T,
    {
        for (k, v) in vars.iter() {
            std::env::set_var(k, v);
        }
        let result = f();
        for (k, _) in vars {
            std::env::remove_var(k);
        }
        result
    }

    #[test]
    fn parse_global() {
        let cmd = "fendermint --network testnet genesis --genesis-file ./genesis.json ipc gateway --subnet-id /r123/t0456 -b 10 -t 10 -f 10 -m 65";
        let opts: GlobalOptions = GlobalOptions::parse_from(cmd.split_ascii_whitespace());
        assert_eq!(opts.global.network, Network::Testnet);
    }

    #[test]
    fn global_options_ignore_help() {
        let cmd = "fendermint --help";
        let _opts: GlobalOptions = GlobalOptions::parse_from(cmd.split_ascii_whitespace());
    }

    #[test]
    fn network_from_env() {
        for (key, _) in ENV_ALIASES.iter() {
            std::env::remove_var(key);
        }

        let examples = [
            (vec![], Network::Mainnet),
            (vec![("IPC_NETWORK", "testnet")], Network::Testnet),
            (vec![("NETWORK", "testnet")], Network::Testnet),
            (vec![("FM_NETWORK", "testnet")], Network::Testnet),
            (
                vec![("IPC_NETWORK", "testnet"), ("FM_NETWORK", "mainnet")],
                Network::Mainnet,
            ),
        ];

        for (i, (vars, network)) in examples.iter().enumerate() {
            let opts = with_env_vars(vars, || {
                set_env_from_aliases();
                let opts: GlobalOptions = GlobalOptions::parse_from(["fendermint", "run"]);
                opts
            });
            assert_eq!(opts.global.network, *network, "example {i}");
        }
    }

    #[test]
    fn options_handle_help() {
        let cmd = "fendermint --help";
        // This test would fail with a panic if we have a misconfiguration in our options.
        // On successfully parsing `--help` with `parse_from` the library would `.exit()` the test framework itself,
        // which is why we must use `try_parse_from`. An error results in a panic from `parse_from` and an `Err`
        // from this, but `--help` is not an `Ok`, since we aren't getting `Options`; it's an `Err` with a help message.
        let e = Options::try_parse_from(cmd.split_ascii_whitespace())
            .expect_err("--help is not Options");

        assert!(e.to_string().contains("Usage:"), "unexpected help: {e}");
    }

    #[test]
    fn parse_log_level() {
        let parse_filter = |cmd: &str| {
            let opts: Options = Options::parse_from(cmd.split_ascii_whitespace());
            opts.log_console_filter().expect("filter should parse")
        };

        let assert_level = |cmd: &str, level: LevelFilter| {
            let filter = parse_filter(cmd);
            assert_eq!(filter.max_level_hint(), Some(level))
        };

        assert_level("fendermint --log-level debug run", LevelFilter::DEBUG);
        assert_level("fendermint --log-level off run", LevelFilter::OFF);
        assert_level(
            "fendermint --log-level libp2p=warn,error run",
            LevelFilter::WARN,
        );
        assert_level("fendermint --log-level info run", LevelFilter::INFO);
    }

    #[test]
    fn parse_invalid_log_level() {
        // NOTE: `nonsense` in itself is interpreted as a target. Maybe we should mandate at least `=` in it?
        let cmd = "fendermint --log-level nonsense/123 run";
        Options::try_parse_from(cmd.split_ascii_whitespace()).expect_err("should not parse");
    }
}
