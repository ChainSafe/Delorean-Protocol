// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Wallet remove cli handler

use async_trait::async_trait;
use clap::Args;
use ipc_wallet::{EvmKeyStore, WalletType};
use std::fmt::Debug;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

pub(crate) struct WalletRemove;

#[async_trait]
impl CommandLineHandler for WalletRemove {
    type Arguments = WalletRemoveArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("remove wallet with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;
        let wallet_type = WalletType::from_str(&arguments.wallet_type)?;

        match wallet_type {
            WalletType::Evm => {
                let wallet = provider.evm_wallet()?;
                let addr = ipc_wallet::EthKeyAddress::from_str(&arguments.address)?;
                wallet.write().unwrap().remove(&addr)?;
            }
            WalletType::Fvm => {
                let wallet = provider.fvm_wallet()?;
                let addr = fvm_shared::address::Address::from_str(&arguments.address)?;
                wallet.write().unwrap().remove(&addr)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Remove wallet from keystore")]
pub(crate) struct WalletRemoveArgs {
    #[arg(long, help = "Address of the key to remove")]
    pub address: String,
    #[arg(long, help = "The type of the wallet, i.e. fvm, evm")]
    pub wallet_type: String,
}
