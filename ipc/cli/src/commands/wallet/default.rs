// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Wallet remove cli handler

use async_trait::async_trait;
use clap::Args;
use ipc_wallet::{EvmKeyStore, WalletType};
use std::fmt::Debug;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

pub(crate) struct WalletSetDefault;

#[async_trait]
impl CommandLineHandler for WalletSetDefault {
    type Arguments = WalletSetDefaultArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("remove wallet with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let wallet_type = WalletType::from_str(&arguments.wallet_type)?;

        match wallet_type {
            WalletType::Evm => {
                let wallet = provider.evm_wallet()?;
                let addr = ipc_wallet::EthKeyAddress::from_str(&arguments.address)?;
                wallet.write().unwrap().set_default(&addr)?;
            }
            WalletType::Fvm => {
                let wallet = provider.fvm_wallet()?;
                let addr = fvm_shared::address::Address::from_str(&arguments.address)?;
                wallet.write().unwrap().set_default(addr)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Set default wallet")]
pub(crate) struct WalletSetDefaultArgs {
    #[arg(long, help = "Address of the key to default")]
    pub address: String,
    #[arg(long, help = "The type of the wallet, i.e. fvm, evm")]
    pub wallet_type: String,
}

pub(crate) struct WalletGetDefault;

#[async_trait]
impl CommandLineHandler for WalletGetDefault {
    type Arguments = WalletGetDefaultArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("remove wallet with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let wallet_type = WalletType::from_str(&arguments.wallet_type)?;

        match wallet_type {
            WalletType::Evm => {
                let wallet = provider.evm_wallet()?;
                let mut wallet = wallet.write().unwrap();
                match wallet.get_default()? {
                    None => println!("No default account set"),
                    Some(addr) => println!("{:?}", addr.to_string()),
                }
            }
            WalletType::Fvm => {
                let wallet = provider.fvm_wallet()?;
                println!("{:?}", wallet.write().unwrap().get_default()?);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Set default wallet")]
pub(crate) struct WalletGetDefaultArgs {
    #[arg(long, help = "The type of the wallet, i.e. fvm, evm")]
    pub wallet_type: String,
}
