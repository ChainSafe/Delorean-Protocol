// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! SendValue cli handler

use async_trait::async_trait;
use clap::Args;
use ipc_api::subnet_id::SubnetID;
use std::{fmt::Debug, str::FromStr};

use crate::{
    f64_to_token_amount, get_ipc_provider, require_fil_addr_from_str, CommandLineHandler,
    GlobalArguments,
};

pub(crate) struct SendValue;

#[async_trait]
impl CommandLineHandler for SendValue {
    type Arguments = SendValueArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("send value in subnet with args: {:?}", arguments);

        let mut provider = get_ipc_provider(global)?;
        let subnet = SubnetID::from_str(&arguments.subnet)?;
        let from = match &arguments.from {
            Some(address) => Some(require_fil_addr_from_str(address)?),
            None => None,
        };

        provider
            .send_value(
                &subnet,
                from,
                require_fil_addr_from_str(&arguments.to)?,
                f64_to_token_amount(arguments.amount)?,
            )
            .await
    }
}

#[derive(Debug, Args)]
#[command(about = "Send value to an address within a subnet")]
pub(crate) struct SendValueArgs {
    #[arg(long, help = "The address to send value from")]
    pub from: Option<String>,
    #[arg(long, help = "The address to send value to")]
    pub to: String,
    #[arg(long, help = "The subnet of the addresses")]
    pub subnet: String,
    #[arg(help = "The amount to send (in whole FIL units)")]
    pub amount: f64,
}
