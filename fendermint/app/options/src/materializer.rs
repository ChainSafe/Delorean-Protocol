// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::{Args, Subcommand};
use fendermint_materializer::{AccountId, TestnetId};

#[derive(Args, Debug)]
pub struct MaterializerArgs {
    /// Path to the directory where the materializer can store its artifacts.
    ///
    /// This must be the same between materializer invocations.
    #[arg(
        long,
        short,
        env = "FM_MATERIALIZER__DATA_DIR",
        default_value = "~/.ipc/materializer"
    )]
    pub data_dir: PathBuf,

    /// Seed for random values in the materialized testnet.
    #[arg(long, short, env = "FM_MATERIALIZER__SEED", default_value = "0")]
    pub seed: u64,

    #[command(subcommand)]
    pub command: MaterializerCommands,
}

#[derive(Subcommand, Debug)]
pub enum MaterializerCommands {
    /// Validate a testnet manifest.
    Validate(MaterializerValidateArgs),
    /// Setup a testnet.
    Setup(MaterializerSetupArgs),
    /// Tear down a testnet.
    #[clap(aliases  = &["teardown", "rm"])]
    Remove(MaterializerRemoveArgs),
    /// Import an existing secret key into a testnet; for example to use an already funded account on Calibration net.
    ImportKey(MaterializerImportKeyArgs),
}

#[derive(Args, Debug)]
pub struct MaterializerValidateArgs {
    /// Path to the manifest file.
    ///
    /// The format of the manifest (e.g. JSON or YAML) will be determined based on the file extension.
    #[arg(long, short)]
    pub manifest_file: PathBuf,
}

#[derive(Args, Debug)]
pub struct MaterializerSetupArgs {
    /// Path to the manifest file.
    ///
    /// The format of the manifest (e.g. JSON or YAML) will be determined based on the file extension.
    ///
    /// The name of the manifest (without the extension) will act as the testnet ID.
    #[arg(long, short)]
    pub manifest_file: PathBuf,

    /// Run validation before attempting to set up the testnet.
    #[arg(long, short, default_value = "false")]
    pub validate: bool,
}

#[derive(Args, Debug)]
pub struct MaterializerRemoveArgs {
    /// ID of the testnet to remove.
    #[arg(long, short)]
    pub testnet_id: TestnetId,
}

#[derive(Args, Debug)]
pub struct MaterializerImportKeyArgs {
    /// Path to the manifest file.
    ///
    /// This is used to determine the testnet ID as well as to check that the account exists.
    #[arg(long, short)]
    pub manifest_file: PathBuf,

    /// Path to the Secp256k1 private key exported in base64 or hexadecimal format.
    #[arg(long, short)]
    pub secret_key: PathBuf,

    /// Run validation before attempting to set up the testnet.
    #[arg(long, short)]
    pub account_id: AccountId,
}
