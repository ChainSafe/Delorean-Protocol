// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! This mod contains the different command line implementations.

mod checkpoint;
mod config;
mod crossmsg;
// mod daemon;
mod subnet;
mod util;
mod wallet;

use crate::commands::checkpoint::CheckpointCommandsArgs;
use crate::commands::crossmsg::CrossMsgsCommandsArgs;
use crate::commands::util::UtilCommandsArgs;
use crate::GlobalArguments;
use anyhow::{anyhow, Context, Result};

use clap::{Command, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use fvm_shared::econ::TokenAmount;
use ipc_api::ethers_address_to_fil_address;

use fvm_shared::address::set_current_network;
use ipc_api::subnet_id::SubnetID;
use ipc_provider::config::{Config, Subnet};
use std::fmt::Debug;
use std::io;
use std::path::Path;
use std::str::FromStr;

use crate::commands::config::ConfigCommandsArgs;
use crate::commands::wallet::WalletCommandsArgs;
use subnet::SubnetCommandsArgs;

/// We only support up to 9 decimal digits for transaction
const FIL_AMOUNT_NANO_DIGITS: u32 = 9;

/// The collection of all subcommands to be called, see clap's documentation for usage. Internal
/// to the current mode. Register a new command accordingly.
#[derive(Debug, Subcommand)]
enum Commands {
    // Daemon(LaunchDaemonArgs),
    Config(ConfigCommandsArgs),
    Subnet(SubnetCommandsArgs),
    Wallet(WalletCommandsArgs),
    CrossMsg(CrossMsgsCommandsArgs),
    Checkpoint(CheckpointCommandsArgs),
    Util(UtilCommandsArgs),
}

#[derive(Debug, Parser)]
#[command(
    name = "ipc-agent",
    about = "The IPC agent command line tool",
    version = "v0.0.1"
)]
#[command(propagate_version = true, arg_required_else_help = true)]
struct IPCAgentCliCommands {
    // If provided, outputs the completion file for given shell
    #[arg(long = "cli-autocomplete-gen", value_enum)]
    generator: Option<Shell>,
    #[clap(flatten)]
    global_params: GlobalArguments,
    #[command(subcommand)]
    command: Option<Commands>,
}

/// A version of options that does partial matching on the arguments, with its only interest
/// being the capture of global parameters that need to take effect first, before we parse [Options],
/// because their value affects how others arse parsed.
///
/// This one doesn't handle `--help` or `help` so that it is passed on to the next parser,
/// where the full set of commands and arguments can be printed properly.
#[derive(Parser, Debug)]
#[command(version, disable_help_flag = true)]
struct GlobalOptions {
    #[command(flatten)]
    global_params: GlobalArguments,

    /// Capture all the normal commands, basically to ingore them.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    pub cmd: Vec<String>,
}

/// The `cli` method exposed to handle all the cli commands, ideally from main.
///
/// # Examples
/// Sample usage:
/// ```ignore
/// # to start the daemon with
/// ipc-client daemon ./config/template.toml
/// ```
///
/// To register a new command, add the command to
/// ```ignore
/// pub async fn cli() {
///
///     // ... other code
///
///     let r = match &args.command {
///         // ... other existing commands
///         Commands::NewCommand => NewCommand::handle(n).await,
///     };
///
///     // ... other code
/// ```
/// Also add this type to Command enum.
/// ```ignore
/// enum Commands {
///     NewCommand(NewCommandArgs),
/// }
/// ```
pub async fn cli() -> anyhow::Result<()> {
    let global = GlobalOptions::parse();
    set_current_network(global.global_params.network());

    // parse the arguments
    let args = IPCAgentCliCommands::parse();

    if let Some(generator) = args.generator {
        let mut cmd = IPCAgentCliCommands::command();
        print_completions(generator, &mut cmd);
        Ok(())
    } else {
        let global = &args.global_params;
        if let Some(c) = &args.command {
            let r = match &c {
                // Commands::Daemon(args) => LaunchDaemon::handle(global, args).await,
                Commands::Config(args) => args.handle(global).await,
                Commands::Subnet(args) => args.handle(global).await,
                Commands::CrossMsg(args) => args.handle(global).await,
                Commands::Wallet(args) => args.handle(global).await,
                Commands::Checkpoint(args) => args.handle(global).await,
                Commands::Util(args) => args.handle(global).await,
            };

            r.with_context(|| format!("error processing command {:?}", args.command))
        } else {
            Ok(())
        }
    }
}

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

pub(crate) fn get_ipc_provider(global: &GlobalArguments) -> Result<ipc_provider::IpcProvider> {
    ipc_provider::IpcProvider::new_from_config(global.config_path())
}

pub(crate) fn f64_to_token_amount(f: f64) -> anyhow::Result<TokenAmount> {
    // no rounding, just the integer part
    let nano = f64::trunc(f * (10u64.pow(FIL_AMOUNT_NANO_DIGITS) as f64));
    Ok(TokenAmount::from_nano(nano as u128))
}

/// Receives a f/eth-address as an input and returns the corresponding
/// filecoin or delegated address, respectively
pub(crate) fn require_fil_addr_from_str(s: &str) -> anyhow::Result<fvm_shared::address::Address> {
    let addr = match fvm_shared::address::Address::from_str(s) {
        Err(_) => {
            // see if it is an eth address
            let addr = ethers::types::Address::from_str(s)?;
            ethers_address_to_fil_address(&addr)?
        }
        Ok(addr) => addr,
    };
    Ok(addr)
}

/// Get the subnet configuration from the config path
pub(crate) fn get_subnet_config(
    config_path: impl AsRef<Path>,
    subnet: &SubnetID,
) -> Result<Subnet> {
    let config = Config::from_file(&config_path)?;
    Ok(config
        .subnets
        .get(subnet)
        .ok_or_else(|| anyhow!("{subnet} is not configured"))?
        .clone())
}

#[cfg(test)]
mod tests {
    use crate::f64_to_token_amount;
    use fvm_shared::econ::TokenAmount;

    #[test]
    fn test_amount() {
        let amount = f64_to_token_amount(1000000.1f64).unwrap();
        assert_eq!(amount, TokenAmount::from_nano(1000000100000000u128));
    }
}
