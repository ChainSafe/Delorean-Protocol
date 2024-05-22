// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Wallet new cli handler

use async_trait::async_trait;
use clap::Args;
use ipc_provider::lotus::message::wallet::WalletKeyType;
use ipc_wallet::WalletType;
use std::fmt::Debug;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

pub(crate) struct WalletNew;

#[async_trait]
impl CommandLineHandler for WalletNew {
    type Arguments = WalletNewArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("create new wallet with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;

        let wallet_type = WalletType::from_str(&arguments.wallet_type)?;
        match wallet_type {
            WalletType::Evm => {
                println!("{:?}", provider.new_evm_key()?.to_string());
            }
            WalletType::Fvm => {
                let tp = WalletKeyType::from_str(
                    &arguments
                        .key_type
                        .clone()
                        .expect("fvm key type not specified"),
                )?;
                println!("{:?}", provider.new_fvm_key(tp)?)
            }
        };

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Create new wallet in subnet")]
pub(crate) struct WalletNewArgs {
    #[arg(
        long,
        help = "The fvm key type of the wallet (secp256k1, bls, secp256k1-ledger), only for fvm wallet type"
    )]
    pub key_type: Option<String>,
    #[arg(long, help = "The type of the wallet, i.e. fvm, evm")]
    pub wallet_type: String,
}
