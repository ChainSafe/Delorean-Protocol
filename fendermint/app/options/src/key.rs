// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum KeyCommands {
    /// Generate a new Secp256k1 key pair and export them to files in base64 format.
    Gen(KeyGenArgs),
    /// Convert a secret key file from base64 into the format expected by Tendermint.
    IntoTendermint(KeyIntoTendermintArgs),
    /// Convert a public key file from base64 into an f1 Address format an print it to STDOUT.
    Address(KeyAddressArgs),
    /// Get the peer ID corresponding to a node ID and its network address and print it to a local file.
    AddPeer(AddPeer),
    /// Converts a hex encoded Ethereum private key into a Base64 encoded Fendermint keypair.
    #[clap(alias = "eth-to-fendermint")]
    FromEth(KeyFromEthArgs),
    /// Converts a Base64 encoded Fendermint private key into a hex encoded Ethereum secret key, public key and address (20 bytes).
    IntoEth(KeyIntoEthArgs),
    /// Show the libp2p peer ID derived from a Secp256k1 public key.
    ShowPeerId(KeyShowPeerIdArgs),
}

#[derive(Args, Debug)]
pub struct KeyArgs {
    #[command(subcommand)]
    pub command: KeyCommands,
}

#[derive(Args, Debug)]
pub struct AddPeer {
    /// The path to a CometBFT node key file.
    #[arg(long, short = 'n')]
    pub node_key_file: PathBuf,
    /// The path to a temporal local file where the peer IDs will be added.
    /// The file will be created if it doesn't exist.
    #[arg(long, short)]
    pub local_peers_file: PathBuf,
    /// The target CometBFT node network interface in the following format `IP:Port`.
    /// For example: `192.168.10.7:26656`.
    #[arg(long, short)]
    pub network_addr: String,
}

#[derive(Args, Debug)]
pub struct KeyGenArgs {
    /// Name used to distinguish the files from other exported keys.
    #[arg(long, short)]
    pub name: String,
    /// Directory to export the key files to; it must exist.
    #[arg(long, short, default_value = ".")]
    pub out_dir: PathBuf,
}

#[derive(Args, Debug)]
pub struct KeyFromEthArgs {
    /// Path to the file that stores the private key (hex format)
    #[arg(long, short)]
    pub secret_key: PathBuf,
    /// Name used to distinguish the files from other exported keys.
    #[arg(long, short)]
    pub name: String,
    /// Directory to export the key files to; it must exist.
    #[arg(long, short, default_value = ".")]
    pub out_dir: PathBuf,
}

#[derive(Args, Debug)]
pub struct KeyIntoEthArgs {
    /// Path to the file that stores the private key (base64 format)
    #[arg(long, short)]
    pub secret_key: PathBuf,
    /// Name used to distinguish the files from other exported keys.
    #[arg(long, short)]
    pub name: String,
    /// Directory to export the key files to; it must exist.
    #[arg(long, short, default_value = ".")]
    pub out_dir: PathBuf,
}

#[derive(Args, Debug)]
pub struct KeyIntoTendermintArgs {
    /// Path to the secret key we want to convert to Tendermint format.
    #[arg(long, short)]
    pub secret_key: PathBuf,
    /// Output file name for the Tendermint private validator key JSON file.
    #[arg(long, short)]
    pub out: PathBuf,
}

#[derive(Args, Debug)]
pub struct KeyAddressArgs {
    /// Path to the public key we want to convert to f1 format.
    #[arg(long, short)]
    pub public_key: PathBuf,
}

#[derive(Args, Debug)]
pub struct KeyShowPeerIdArgs {
    /// Path to the public key we want to convert to a libp2p peer ID.
    #[arg(long, short)]
    pub public_key: PathBuf,
}
