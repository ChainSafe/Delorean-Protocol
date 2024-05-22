// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use crate::{CommandLineHandler, GlobalArguments};

use crate::commands::wallet::balances::{WalletBalances, WalletBalancesArgs};
use crate::commands::wallet::new::{WalletNew, WalletNewArgs};
use clap::{Args, Subcommand};

use self::default::{
    WalletGetDefault, WalletGetDefaultArgs, WalletSetDefault, WalletSetDefaultArgs,
};
use self::export::{WalletExport, WalletExportArgs, WalletPublicKey, WalletPublicKeyArgs};
use self::import::{WalletImport, WalletImportArgs};
use self::list::{WalletList, WalletListArgs};
use self::remove::{WalletRemove, WalletRemoveArgs};

mod balances;
mod default;
mod export;
mod import;
mod list;
mod new;
mod remove;

#[derive(Debug, Args)]
#[command(name = "wallet", about = "wallet related commands")]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct WalletCommandsArgs {
    #[command(subcommand)]
    command: Commands,
}

impl WalletCommandsArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::New(args) => WalletNew::handle(global, args).await,
            Commands::Balances(args) => WalletBalances::handle(global, args).await,
            Commands::Import(args) => WalletImport::handle(global, args).await,
            Commands::Export(args) => WalletExport::handle(global, args).await,
            Commands::Remove(args) => WalletRemove::handle(global, args).await,
            Commands::SetDefault(args) => WalletSetDefault::handle(global, args).await,
            Commands::GetDefault(args) => WalletGetDefault::handle(global, args).await,
            Commands::PubKey(args) => WalletPublicKey::handle(global, args).await,
            Commands::List(args) => WalletList::handle(global, args).await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    New(WalletNewArgs),
    Balances(WalletBalancesArgs),
    Import(WalletImportArgs),
    Export(WalletExportArgs),
    Remove(WalletRemoveArgs),
    SetDefault(WalletSetDefaultArgs),
    GetDefault(WalletGetDefaultArgs),
    PubKey(WalletPublicKeyArgs),
    List(WalletListArgs),
}
