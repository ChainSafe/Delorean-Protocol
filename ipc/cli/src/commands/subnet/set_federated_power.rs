// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Set federated power cli handler

use crate::commands::{get_ipc_provider, require_fil_addr_from_str};
use crate::{CommandLineHandler, GlobalArguments};
use async_trait::async_trait;
use clap::Args;
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;
use std::str::FromStr;

/// The command to set federated power.
pub struct SetFederatedPower;

#[async_trait]
impl CommandLineHandler for crate::commands::subnet::SetFederatedPower {
    type Arguments = crate::commands::subnet::SetFederatedPowerArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("set federated power with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;

        let addresses: Vec<Address> = arguments
            .validator_addresses
            .iter()
            .map(|address| require_fil_addr_from_str(address).unwrap())
            .collect();

        let public_keys: Vec<Vec<u8>> = arguments
            .validator_pubkeys
            .iter()
            .map(|key| hex::decode(key).unwrap())
            .collect();

        let from_address = require_fil_addr_from_str(&arguments.from).unwrap();

        let chain_epoch = provider
            .set_federated_power(
                &from_address,
                &subnet,
                &addresses,
                &public_keys,
                &arguments.validator_power,
            )
            .await?;
        println!("New federated power is set at epoch {chain_epoch}");

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(
    name = "set-federated-power",
    about = "Set federated power for validators"
)]
pub struct SetFederatedPowerArgs {
    #[arg(long, help = "The address to sign and pay for this transaction.")]
    pub from: String,
    #[arg(long, help = "The subnet to release collateral from")]
    pub subnet: String,
    #[arg(long, num_args = 1.., help = "Addresses of validators, separated by space")]
    pub validator_addresses: Vec<String>,
    #[arg(long, num_args = 1.., help = "Public keys of validators, separated by space")]
    pub validator_pubkeys: Vec<String>,
    #[arg(long, num_args = 1.., help = "Federated of validators, separated by space")]
    pub validator_power: Vec<u128>,
}
