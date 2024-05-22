// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Example of using the RPC library to send tokens from an f410 account to an f1 account.
//!
//! The example assumes that Tendermint and Fendermint have been started
//! and are running locally.
//!
//! # Usage
//! ```text
//! cargo run -p fendermint_rpc --release --example transfer -- --secret-key test-network/keys/eric.sk --verbose
//! ```

use std::path::PathBuf;

use anyhow::{anyhow, Context};
use clap::Parser;
use fendermint_rpc::query::QueryClient;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_message::query::FvmQueryHeight;
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

    /// Path to the secret key to deploy with, expected to be in Base64 format,
    /// and that it has a corresponding f410 account in genesis.
    #[arg(long, short)]
    pub secret_key: PathBuf,
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
    let f410_addr = Address::from(EthAddress::from(pk));

    // Query the account nonce from the state, so it doesn't need to be passed as an arg.
    let sn = sequence(&client, &f410_addr)
        .await
        .expect("error getting sequence");

    // Query the chain ID, so it doesn't need to be passed as an arg.
    let chain_id = client
        .state_params(FvmQueryHeight::default())
        .await
        .expect("error getting state params")
        .value
        .chain_id;

    let mf = SignedMessageFactory::new(sk, f410_addr, sn, ChainID::from(chain_id));

    let mut client = client.bind(mf);

    let res = TxClient::<TxCommit>::transfer(
        &mut client,
        f1_addr,
        TokenAmount::from_whole(1),
        GAS_PARAMS.clone(),
    )
    .await
    .expect("transfer failed");

    assert!(res.response.check_tx.code.is_ok(), "check is ok");
    assert!(res.response.deliver_tx.code.is_ok(), "deliver is ok");
    assert!(res.return_data.is_some());
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
