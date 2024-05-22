// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use ipc_api::subnet_id::SubnetID;

use super::parse::{
    parse_eth_address, parse_full_fil, parse_network_version, parse_percentage, parse_signer_addr,
    parse_token_amount,
};
use fendermint_vm_genesis::SignerAddr;
use fvm_shared::{address::Address, econ::TokenAmount, version::NetworkVersion};

#[derive(Debug, Clone, ValueEnum)]
pub enum AccountKind {
    Regular,
    Ethereum,
}

#[derive(Subcommand, Debug)]
pub enum GenesisCommands {
    /// Create a new Genesis file, with accounts and validators to be added later.
    New(GenesisNewArgs),
    /// Add an account to the genesis file.
    AddAccount(GenesisAddAccountArgs),
    /// Add a multi-sig account to the genesis file.
    AddMultisig(GenesisAddMultisigArgs),
    /// Add a validator to the genesis file.
    AddValidator(GenesisAddValidatorArgs),
    /// Set the EAM actor permission mode.
    SetEamPermissions(GenesisSetEAMPermissionsArgs),
    /// IPC commands.
    Ipc {
        #[command(subcommand)]
        command: GenesisIpcCommands,
    },
    /// Convert the genesis file into the format expected by Tendermint.
    IntoTendermint(GenesisIntoTendermintArgs),
}

#[derive(Args, Debug)]
pub struct GenesisSetEAMPermissionsArgs {
    #[arg(
        long,
        short,
        default_value = "unrestricted",
        help = "Permission mode (unrestricted/allowlist) that controls who can deploy contracts in the subnet"
    )]
    pub mode: String,

    #[arg(
        long,
        short,
        value_delimiter = ',',
        value_parser = parse_signer_addr,
        help = "List of addresses that can deploy contract. Field is ignored if mode is unrestricted"
    )]
    pub addresses: Vec<SignerAddr>,
}

#[derive(Args, Debug)]
pub struct GenesisArgs {
    /// Path to the genesis JSON file.
    #[arg(long, short)]
    pub genesis_file: PathBuf,

    #[command(subcommand)]
    pub command: GenesisCommands,
}

#[derive(Args, Debug)]
pub struct GenesisNewArgs {
    /// Genesis timestamp as seconds since Unix epoch.
    #[arg(long, short)]
    pub timestamp: u64,
    /// Name of the network and chain.
    #[arg(long, short = 'n')]
    pub chain_name: String,
    /// Network version, governs which set of built-in actors to use.
    #[arg(long, short = 'v', default_value = "21", value_parser = parse_network_version)]
    pub network_version: NetworkVersion,
    /// Base fee for running transactions in atto.
    #[arg(long, short = 'f', value_parser = parse_token_amount)]
    pub base_fee: TokenAmount,
    /// Number of decimals to use during converting FIL to Power.
    #[arg(long, short)]
    pub power_scale: i8,
}

#[derive(Args, Debug)]
pub struct GenesisAddAccountArgs {
    /// Path to the Secp256k1 public key exported in base64 format.
    #[arg(long, short)]
    pub public_key: PathBuf,
    /// Initial balance in full FIL units.
    #[arg(long, short, value_parser = parse_full_fil)]
    pub balance: TokenAmount,
    /// Indicate whether the account is a regular or ethereum account.
    #[arg(long, short, default_value = "regular")]
    pub kind: AccountKind,
}

#[derive(Args, Debug)]
pub struct GenesisAddMultisigArgs {
    /// Path to the Secp256k1 public key exported in base64 format, one for each signatory.
    #[arg(long, short)]
    pub public_key: Vec<PathBuf>,
    /// Initial balance in full FIL units.
    #[arg(long, short, value_parser = parse_full_fil)]
    pub balance: TokenAmount,
    /// Number of signatures required.
    #[arg(long, short)]
    pub threshold: u64,
    /// Linear unlock duration in block heights.
    #[arg(long, short = 'd')]
    pub vesting_duration: u64,
    /// Linear unlock start block height.
    #[arg(long, short = 's')]
    pub vesting_start: u64,
}

#[derive(Args, Debug)]
pub struct GenesisAddValidatorArgs {
    /// Path to the Secp256k1 public key exported in base64 format.
    #[arg(long, short)]
    pub public_key: PathBuf,
    /// The collateral staked by the validator, lending it its voting power.
    #[arg(long, short = 'v', value_parser = parse_full_fil)]
    pub power: TokenAmount,
}

#[derive(Args, Debug)]
pub struct GenesisIntoTendermintArgs {
    /// Output file name for the Tendermint genesis JSON file.
    #[arg(long, short)]
    pub out: PathBuf,
    /// Maximum block size in bytes.
    #[arg(long, default_value_t = 22020096)]
    pub block_max_bytes: u64,
}

#[derive(Subcommand, Debug, Clone)]
pub enum GenesisIpcCommands {
    /// Set all gateway parameters.
    Gateway(GenesisIpcGatewayArgs),
    /// Fetch the genesis parameters of a subnet from the parent.
    FromParent(Box<GenesisFromParentArgs>),
}

#[derive(Args, Debug, Clone)]
pub struct GenesisIpcGatewayArgs {
    /// Set the current subnet ID, which is the path from the root to the subnet actor in the parent.
    #[arg(long, short)]
    pub subnet_id: SubnetID,

    #[arg(long, short)]
    pub bottom_up_check_period: u64,

    /// Message fee in atto.
    #[arg(long, short = 'f', value_parser = parse_token_amount)]
    pub msg_fee: TokenAmount,

    /// Quorum majority percentage [51 - 100]
    #[arg(long, short, value_parser = parse_percentage::<u8>)]
    pub majority_percentage: u8,

    /// Maximum number of active validators.
    #[arg(long, short = 'v', default_value = "100")]
    pub active_validators_limit: u16,
}

#[derive(Args, Debug, Clone)]
pub struct GenesisFromParentArgs {
    /// Child subnet for with the genesis file is being created
    #[arg(long, short)]
    pub subnet_id: SubnetID,

    /// Endpoint to the RPC of the child subnet's parent
    #[arg(long, short)]
    pub parent_endpoint: url::Url,

    /// HTTP basic authentication token.
    #[arg(long)]
    pub parent_auth_token: Option<String>,

    /// IPC gateway of the parent; 20 byte Ethereum address in 0x prefixed hex format
    #[arg(long, value_parser = parse_eth_address, default_value = "0xff00000000000000000000000000000000000064")]
    pub parent_gateway: Address,

    /// IPC registry of the parent; 20 byte Ethereum address in 0x prefixed hex format
    #[arg(long, value_parser = parse_eth_address, default_value = "0xff00000000000000000000000000000000000065")]
    pub parent_registry: Address,

    /// Network version, governs which set of built-in actors to use.
    #[arg(long, short = 'v', default_value = "21", value_parser = parse_network_version)]
    pub network_version: NetworkVersion,

    /// Base fee for running transactions in atto.
    #[arg(long, short = 'f', value_parser = parse_token_amount, default_value = "1000")]
    pub base_fee: TokenAmount,

    /// Number of decimals to use during converting FIL to Power.
    #[arg(long, default_value = "3")]
    pub power_scale: i8,
}
