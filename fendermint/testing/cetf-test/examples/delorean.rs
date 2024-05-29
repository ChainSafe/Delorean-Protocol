// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helper commands for interacting with the Delorean/CETF actor via RPC
//!
//! The example assumes that Tendermint and Fendermint have been started
//! and are running locally.
//!
//! # Usage
//! ```text
//! cargo run -p cetf_tests --example delorean -- register-bls --secret-key test-network/keys/eric.sk --bls-secret-key test-network/keys/eric.bls.sk --verbose
//! ```

use std::path::PathBuf;

use anyhow::{anyhow, Context};
use bls_signatures::Serialize;
use clap::{Parser, Subcommand};
use fendermint_actor_cetf as cetf_actor;
use fendermint_rpc::query::QueryClient;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::chainid::ChainID;
use lazy_static::lazy_static;
use tendermint_rpc::Url;
use tracing::Level;

use fvm_shared::econ::TokenAmount;

use fendermint_rpc::client::FendermintClient;
use fendermint_rpc::message::{GasParams, SignedMessageFactory};
use fendermint_rpc::tx::{TxClient, TxCommit};

lazy_static! {
    /// Default gas params based on the testkit.
    static ref GAS_PARAMS: GasParams = GasParams {
        gas_limit: 10_000_000_000,
        gas_fee_cap: TokenAmount::default(),
        gas_premium: TokenAmount::default(),
    };
}

#[derive(Parser, Debug)]
pub struct Options {
    /// The URL of the Tendermint node's RPC endpoint.
    #[arg(
        long,
        short,
        default_value = "http://127.0.0.1:26657",
        env = "TENDERMINT_RPC_URL"
    )]
    pub url: Url,

    /// Enable DEBUG logs.
    #[arg(long, short)]
    pub verbose: bool,

    #[command(subcommand)]
    command: Commands,

    /// Path to the secret key to deploy with, expected to be in Base64 format,
    /// and that it has a corresponding f410 account in genesis.
    #[arg(long, short)]
    pub secret_key: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Commands {
    RegisterBls {
        #[arg(long, short)]
        bls_secret_key: PathBuf,
    },
    QueueTag,
}

impl Options {
    pub fn log_level(&self) -> Level {
        if self.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        }
    }
}

/// See the module docs for how to run.
#[tokio::main]
async fn main() {
    let opts: Options = Options::parse();

    tracing_subscriber::fmt()
        .with_max_level(opts.log_level())
        .init();

    let client = FendermintClient::new_http(opts.url, None).expect("error creating client");

    let sk =
        SignedMessageFactory::read_secret_key(&opts.secret_key).expect("error reading secret key");

    let pk = sk.public_key();

    let f1_addr = Address::new_secp256k1(&pk.serialize()).expect("valid public key");

    // Query the account nonce from the state, so it doesn't need to be passed as an arg.
    let sn = sequence(&client, &f1_addr)
        .await
        .expect("error getting sequence");

    // Query the chain ID, so it doesn't need to be passed as an arg.
    let chain_id = client
        .state_params(FvmQueryHeight::default())
        .await
        .expect("error getting state params")
        .value
        .chain_id;

    let mf = SignedMessageFactory::new(sk, f1_addr, sn, ChainID::from(chain_id));

    let mut client = client.bind(mf);

    match opts.command {
        Commands::RegisterBls { bls_secret_key } => {
            let bls_sk = {
                let b64 = std::fs::read_to_string(&bls_secret_key)
                    .expect("failed to read bls secret key");
                bls_signatures::PrivateKey::from_bytes(
                    &fendermint_crypto::from_b64(&b64)
                        .expect("failed to decode b64 bls secret key"),
                )
                .expect("failed to parse bls secret key")
            };

            let bls_pk = bls_sk.public_key();

            let res = TxClient::<TxCommit>::transaction(
                &mut client,
                fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR,
                cetf_actor::Method::AddValidator as u64,
                RawBytes::serialize(cetf_actor::AddValidatorParams {
                    address: f1_addr,
                    public_key: fendermint_actor_cetf::BlsPublicKey(
                        bls_pk
                            .as_bytes()
                            .try_into()
                            .expect("Failed to convert BLS public key to bytes"),
                    ),
                })
                .expect("failed to serialize add validator params"),
                TokenAmount::from_whole(0),
                GAS_PARAMS.clone(),
            )
            .await
            .expect("transfer failed");

            assert!(res.response.check_tx.code.is_ok(), "check is ok");
            assert!(res.response.tx_result.code.is_ok(), "deliver is ok");
            assert!(res.return_data.is_some());
        }
        Commands::QueueTag => {
            todo!();
        }
    }
}

/// Get the next sequence number (nonce) of an account.
async fn sequence(client: &impl QueryClient, addr: &Address) -> anyhow::Result<u64> {
    let state = client
        .actor_state(&addr, FvmQueryHeight::default())
        .await
        .context("failed to get actor state")?;

    match state.value {
        Some((_id, state)) => Ok(state.sequence),
        None => Err(anyhow!("cannot find actor {addr}")),
    }
}
