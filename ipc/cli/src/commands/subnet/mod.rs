// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

pub use crate::commands::subnet::create::{CreateSubnet, CreateSubnetArgs};
use crate::commands::subnet::genesis_epoch::{GenesisEpoch, GenesisEpochArgs};
pub use crate::commands::subnet::join::{JoinSubnet, JoinSubnetArgs};
pub use crate::commands::subnet::kill::{KillSubnet, KillSubnetArgs};
pub use crate::commands::subnet::leave::{LeaveSubnet, LeaveSubnetArgs};
use crate::commands::subnet::list_subnets::{ListSubnets, ListSubnetsArgs};
use crate::commands::subnet::rpc::{RPCSubnet, RPCSubnetArgs};
use crate::commands::subnet::send_value::{SendValue, SendValueArgs};
use crate::commands::subnet::set_federated_power::{SetFederatedPower, SetFederatedPowerArgs};
use crate::commands::subnet::show_gateway_contract_commit_sha::{
    ShowGatewayContractCommitSha, ShowGatewayContractCommitShaArgs,
};
use crate::commands::subnet::validator::{ValidatorInfo, ValidatorInfoArgs};
use crate::{CommandLineHandler, GlobalArguments};
use clap::{Args, Subcommand};

use self::bootstrap::{AddBootstrap, AddBootstrapArgs, ListBootstraps, ListBootstrapsArgs};
use self::join::{StakeSubnet, StakeSubnetArgs, UnstakeSubnet, UnstakeSubnetArgs};
use self::leave::{Claim, ClaimArgs};
use self::rpc::{ChainIdSubnet, ChainIdSubnetArgs};

pub mod bootstrap;
pub mod create;
mod genesis_epoch;
pub mod join;
pub mod kill;
pub mod leave;
pub mod list_subnets;
pub mod rpc;
pub mod send_value;
mod set_federated_power;
pub mod show_gateway_contract_commit_sha;
mod validator;

#[derive(Debug, Args)]
#[command(
    name = "subnet",
    about = "subnet related commands such as create, join and etc"
)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct SubnetCommandsArgs {
    #[command(subcommand)]
    command: Commands,
}

impl SubnetCommandsArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::Create(args) => CreateSubnet::handle(global, args).await,
            Commands::List(args) => ListSubnets::handle(global, args).await,
            Commands::Join(args) => JoinSubnet::handle(global, args).await,
            Commands::Rpc(args) => RPCSubnet::handle(global, args).await,
            Commands::ChainId(args) => ChainIdSubnet::handle(global, args).await,
            Commands::Leave(args) => LeaveSubnet::handle(global, args).await,
            Commands::Kill(args) => KillSubnet::handle(global, args).await,
            Commands::SendValue(args) => SendValue::handle(global, args).await,
            Commands::Stake(args) => StakeSubnet::handle(global, args).await,
            Commands::Unstake(args) => UnstakeSubnet::handle(global, args).await,
            Commands::Claim(args) => Claim::handle(global, args).await,
            Commands::AddBootstrap(args) => AddBootstrap::handle(global, args).await,
            Commands::ListBootstraps(args) => ListBootstraps::handle(global, args).await,
            Commands::GenesisEpoch(args) => GenesisEpoch::handle(global, args).await,
            Commands::GetValidator(args) => ValidatorInfo::handle(global, args).await,
            Commands::ShowGatewayContractCommitSha(args) => {
                ShowGatewayContractCommitSha::handle(global, args).await
            }
            Commands::SetFederatedPower(args) => SetFederatedPower::handle(global, args).await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    Create(CreateSubnetArgs),
    List(ListSubnetsArgs),
    Join(JoinSubnetArgs),
    Rpc(RPCSubnetArgs),
    ChainId(ChainIdSubnetArgs),
    Leave(LeaveSubnetArgs),
    Kill(KillSubnetArgs),
    SendValue(SendValueArgs),
    Stake(StakeSubnetArgs),
    Unstake(UnstakeSubnetArgs),
    Claim(ClaimArgs),
    AddBootstrap(AddBootstrapArgs),
    ListBootstraps(ListBootstrapsArgs),
    GenesisEpoch(GenesisEpochArgs),
    GetValidator(ValidatorInfoArgs),
    ShowGatewayContractCommitSha(ShowGatewayContractCommitShaArgs),
    SetFederatedPower(SetFederatedPowerArgs),
}
