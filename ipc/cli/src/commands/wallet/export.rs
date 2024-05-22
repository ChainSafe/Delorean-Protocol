// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Wallet export cli handler
use anyhow::anyhow;
use async_trait::async_trait;
use base64::{prelude::BASE64_STANDARD, Engine};
use clap::Args;
use fvm_shared::address::Address;
use ipc_provider::{lotus::message::wallet::WalletKeyType, IpcProvider, LotusJsonKeyType};
use ipc_wallet::{EvmKeyStore, PersistentKeyInfo, WalletType};
use std::fmt::Debug;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::str::FromStr;

use crate::{get_ipc_provider, CommandLineHandler, GlobalArguments};

pub(crate) struct WalletExport;

impl WalletExport {
    fn export_evm(provider: &IpcProvider, arguments: &WalletExportArgs) -> anyhow::Result<String> {
        let keystore = provider.evm_wallet()?;
        let address = ethers::types::Address::from_str(&arguments.address)?;

        let key_info = keystore
            .read()
            .unwrap()
            .get(&address.into())?
            .ok_or_else(|| anyhow!("key does not exists"))?;

        if arguments.hex {
            return Ok(hex::encode(key_info.private_key()));
        }

        if arguments.fendermint {
            return Ok(BASE64_STANDARD.encode(key_info.private_key()));
        }

        let info = PersistentKeyInfo::new(
            format!("{:?}", address),
            hex::encode(key_info.private_key()),
        );
        Ok(serde_json::to_string(&info)?)
    }

    fn export_fvm(provider: &IpcProvider, arguments: &WalletExportArgs) -> anyhow::Result<String> {
        let wallet = provider.fvm_wallet()?;

        let addr = Address::from_str(&arguments.address)?;
        let key_info = wallet.write().unwrap().export(&addr)?;

        if arguments.hex {
            return Ok(hex::encode(key_info.private_key()));
        }

        if arguments.fendermint {
            return Ok(BASE64_STANDARD.encode(key_info.private_key()));
        }

        Ok(serde_json::to_string(&LotusJsonKeyType {
            r#type: WalletKeyType::try_from(*key_info.key_type())?.to_string(),
            private_key: BASE64_STANDARD.encode(key_info.private_key()),
        })?)
    }
}

#[async_trait]
impl CommandLineHandler for WalletExport {
    type Arguments = WalletExportArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("export wallet with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;

        let wallet_type = WalletType::from_str(&arguments.wallet_type)?;
        let v = match wallet_type {
            WalletType::Evm => WalletExport::export_evm(&provider, arguments),
            WalletType::Fvm => WalletExport::export_fvm(&provider, arguments),
        }?;

        match &arguments.output {
            Some(p) => {
                let mut file = std::fs::File::create(p)?;
                file.set_permissions(Permissions::from_mode(0o600))?;
                file.write_all(v.as_bytes())?;
                println!(
                    "exported new wallet with address {:?} in file {:?}",
                    arguments.address, p
                );
            }
            None => {
                println!("{}", v);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Export the key from a wallet address in JSON format")]
pub(crate) struct WalletExportArgs {
    #[arg(long, help = "Address of the key to export")]
    pub address: String,
    #[arg(
        long,
        help = "Optional parameter that outputs the address key into the file specified"
    )]
    pub output: Option<String>,
    #[arg(long, help = "The type of the wallet, i.e. fvm, evm")]
    pub wallet_type: String,
    #[arg(
        long,
        help = "Exports the secret key encoded in base64 as Fendermint expects"
    )]
    pub fendermint: bool,
    #[arg(long, help = "Export the hex encoded secret key")]
    pub hex: bool,
}

pub(crate) struct WalletPublicKey;

impl WalletPublicKey {
    fn pubkey_evm(
        provider: &IpcProvider,
        arguments: &WalletPublicKeyArgs,
    ) -> anyhow::Result<String> {
        let keystore = provider.evm_wallet()?;
        let address = ethers::types::Address::from_str(&arguments.address)?;

        let key_info = keystore
            .read()
            .unwrap()
            .get(&address.into())?
            .ok_or_else(|| anyhow!("key does not exists"))?;

        let sk = libsecp256k1::SecretKey::parse_slice(key_info.private_key())?;
        Ok(hex::encode(libsecp256k1::PublicKey::from_secret_key(&sk).serialize()).to_string())
    }

    fn pubkey_fvm(
        provider: &IpcProvider,
        arguments: &WalletPublicKeyArgs,
    ) -> anyhow::Result<String> {
        let wallet = provider.fvm_wallet()?;

        let addr = Address::from_str(&arguments.address)?;
        let key_info = wallet.write().unwrap().export(&addr)?;

        let sk = libsecp256k1::SecretKey::parse_slice(key_info.private_key())?;
        Ok(hex::encode(libsecp256k1::PublicKey::from_secret_key(&sk).serialize()).to_string())
    }
}

#[async_trait]
impl CommandLineHandler for WalletPublicKey {
    type Arguments = WalletPublicKeyArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("export wallet with args: {:?}", arguments);

        let provider = get_ipc_provider(global)?;

        let wallet_type = WalletType::from_str(&arguments.wallet_type)?;
        let v = match wallet_type {
            WalletType::Evm => WalletPublicKey::pubkey_evm(&provider, arguments),
            WalletType::Fvm => WalletPublicKey::pubkey_fvm(&provider, arguments),
        }?;
        println!("{v}");
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Return public key from a wallet address")]
pub(crate) struct WalletPublicKeyArgs {
    #[arg(long, help = "Address of the key to export")]
    pub address: String,
    #[arg(long, help = "The type of the wallet, i.e. fvm, evm")]
    pub wallet_type: String,
}
