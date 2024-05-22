// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Fund cli command handler.

use async_trait::async_trait;
use clap::Args;
use fvm_shared::bigint::BigInt;
use fvm_shared::econ::TokenAmount;
use ipc_api::subnet_id::SubnetID;
use num_traits::Num;
use std::{fmt::Debug, str::FromStr};

use crate::{
    f64_to_token_amount, get_ipc_provider, require_fil_addr_from_str, CommandLineHandler,
    GlobalArguments,
};

/// The command to send funds to a subnet from parent
pub(crate) struct Fund;

#[async_trait]
impl CommandLineHandler for Fund {
    type Arguments = FundArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("fund operation with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };
        let to = match &arguments.to {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };
        let gateway_addr = match &arguments.gateway_address {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        println!(
            "fund performed in epoch: {:?}",
            provider
                .fund(
                    subnet,
                    gateway_addr,
                    from,
                    to,
                    f64_to_token_amount(arguments.amount)?,
                )
                .await?,
        );

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Send funds from a parent to a child subnet")]
pub(crate) struct FundArgs {
    #[arg(long, help = "The gateway address of the subnet")]
    pub gateway_address: Option<String>,
    #[arg(long, help = "The address to send funds from")]
    pub from: Option<String>,
    #[arg(
        long,
        help = "The address to send funds to (if not set, amount sent to from address)"
    )]
    pub to: Option<String>,
    #[arg(long, help = "The subnet to fund")]
    pub subnet: String,
    #[arg(help = "The amount to fund in FIL, in whole FIL")]
    pub amount: f64,
}

pub struct PreFund;

#[async_trait]
impl CommandLineHandler for PreFund {
    type Arguments = PreFundArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("pre-fund subnet with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };
        provider
            .pre_fund(
                subnet.clone(),
                from,
                f64_to_token_amount(arguments.initial_balance)?,
            )
            .await?;
        log::info!("address pre-funded successfully");

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(
    name = "pre-fund",
    about = "Add some funds in genesis to an address in a child-subnet"
)]
pub struct PreFundArgs {
    #[arg(long, help = "The address funded in the subnet")]
    pub from: Option<String>,
    #[arg(long, help = "The subnet to add balance to")]
    pub subnet: String,
    #[arg(help = "Add an initial balance for the address in genesis in the subnet")]
    pub initial_balance: f64,
}

/// The command to send ERC20 tokens to a subnet from parent
pub(crate) struct FundWithToken;

#[async_trait]
impl CommandLineHandler for FundWithToken {
    type Arguments = FundWithTokenArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("fund with token operation with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };
        let to = match &arguments.to {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        let amount = BigInt::from_str_radix(arguments.amount.as_str(), 10)
            .map_err(|e| anyhow::anyhow!("not a token amount: {e}"))
            .map(TokenAmount::from_atto)?;

        if arguments.approve {
            println!(
                "approve token performed in epoch: {:?}",
                provider
                    .approve_token(subnet.clone(), from, amount.clone())
                    .await?,
            );
        }

        println!(
            "fund with token performed in epoch: {:?}",
            provider.fund_with_token(subnet, from, to, amount).await?,
        );

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Send erc20 tokens from a parent to a child subnet")]
pub(crate) struct FundWithTokenArgs {
    #[arg(long, help = "The address to send funds from")]
    pub from: Option<String>,
    #[arg(
        long,
        help = "The address to send funds to (if not set, amount sent to from address)"
    )]
    pub to: Option<String>,
    #[arg(long, help = "The subnet to fund")]
    pub subnet: String,
    #[arg(help = "The amount to fund in erc20, in the token's precision unit")]
    pub amount: String,
    #[arg(long, help = "Approve gateway before funding")]
    pub approve: bool,
}
