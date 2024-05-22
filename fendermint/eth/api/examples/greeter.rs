// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Example of using the Ethereum JSON-RPC facade with the Ethers provider.
//!
//! The example assumes that the following has been started and running in the background:
//! 1. Fendermint ABCI application
//! 2. Tendermint Core / Comet BFT
//! 3. Fendermint Ethereum API facade
//!
//! # Usage
//! ```text
//! cargo run -p fendermint_eth_api --release --example GREETER --
//! ```

use std::{fmt::Debug, path::PathBuf, sync::Arc};

use anyhow::Context;
use clap::Parser;
use ethers::contract::LogMeta;
use ethers::{
    prelude::{abigen, ContractFactory},
    providers::{Http, JsonRpcClient, Middleware, Provider},
};
use ethers_core::{
    abi::Abi,
    types::{Bytes, TransactionReceipt},
};
use serde_json::json;
use tracing::Level;

use crate::common::{adjust_provider, make_middleware, TestAccount, TestContractCall};

#[allow(dead_code)]
mod common;

// Generate a statically typed interface for the contract.
abigen!(Greeter, "../../testing/contracts/Greeter.abi");

const GREETER_HEX: &'static str = include_str!("../../../testing/contracts/Greeter.bin");

#[derive(Parser, Debug)]
pub struct Options {
    /// The host of the Fendermint Ethereum API endpoint.
    #[arg(long, default_value = "127.0.0.1", env = "FM_ETH__LISTEN__HOST")]
    pub http_host: String,

    /// The port of the Fendermint Ethereum API endpoint.
    #[arg(long, default_value = "8545", env = "FM_ETH__LISTEN__PORT")]
    pub http_port: u32,

    /// Secret key used to deploy the contract.
    ///
    /// Assumed to exist with a non-zero balance.
    #[arg(long, short)]
    pub secret_key: PathBuf,

    /// Path to write the contract metadata to.
    #[arg(long, short)]
    pub out: Option<PathBuf>,

    /// Enable DEBUG logs.
    #[arg(long, short)]
    pub verbose: bool,
}

impl Options {
    pub fn log_level(&self) -> Level {
        if self.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        }
    }

    pub fn http_endpoint(&self) -> String {
        format!("http://{}:{}", self.http_host, self.http_port)
    }
}

/// See the module docs for how to run.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts: Options = Options::parse();

    tracing_subscriber::fmt()
        .with_max_level(opts.log_level())
        .init();

    let provider = Provider::<Http>::try_from(opts.http_endpoint())?;

    run_http(provider, &opts).await?;

    Ok(())
}

async fn run<C>(provider: &Provider<C>, opts: &Options) -> anyhow::Result<()>
where
    C: JsonRpcClient + Clone + 'static,
{
    let from = TestAccount::new(&opts.secret_key)?;

    tracing::info!(from = ?from.eth_addr, "ethereum address");
    tracing::info!("deploying Greeter");

    let bytecode = Bytes::from(hex::decode(GREETER_HEX).context("failed to decode contract hex")?);
    let abi: Abi = GREETER_ABI.clone();

    let chain_id = provider.get_chainid().await?;

    let mw = make_middleware(provider.clone(), chain_id.as_u64(), &from)
        .context("failed to create middleware")?;

    let mw = Arc::new(mw);

    const GREETING0: &str = "Welcome, weary traveller!";
    const GREETING1: &str = "Howdy doody!";

    let factory = ContractFactory::new(abi, bytecode.clone(), mw.clone());
    let deployer = factory.deploy((GREETING0.to_string(),))?;

    let (contract, deploy_receipt): (_, TransactionReceipt) = deployer
        .send_with_receipt()
        .await
        .context("failed to send deployment")?;

    tracing::info!(addr = ?contract.address(), "Greeter deployed");

    let contract = Greeter::new(contract.address(), contract.client());

    let greeting: String = contract
        .greet()
        .call()
        .await
        .context("failed to call greet")?;

    assert_eq!(greeting, GREETING0);

    let deploy_height = deploy_receipt.block_number.expect("deploy height is known");

    // Set the greeting to emit an event.
    let set_greeting: TestContractCall<_, ()> = contract.set_greeting(GREETING1.to_string());

    let _tx_receipt: TransactionReceipt = set_greeting
        .send()
        .await
        .context("failed to set greeting")?
        .log_msg("set_greeting")
        .retries(3)
        .await?
        .context("cannot get receipt")?;

    let greeting: String = contract
        .greet()
        .call()
        .await
        .context("failed to call greet")?;

    assert_eq!(greeting, GREETING1);

    let logs: Vec<(GreetingSetFilter, LogMeta)> = contract
        .greeting_set_filter()
        .address(contract.address().into())
        .from_block(deploy_height)
        .query_with_meta()
        .await
        .context("failed to query logs")?;

    assert_eq!(logs.len(), 2, "events: constructor + invocation");
    assert_eq!(logs[0].0.greeting, GREETING0);
    assert_eq!(logs[1].0.greeting, GREETING1);

    if let Some(ref out) = opts.out {
        // Print some metadata so that we can configure The Graph:
        // `subgraph.template.yaml` requires the `address` and `startBlock` to be configured.
        let output = json!({
            "address": format!("{:?}", contract.address()),
            "deploy_height": deploy_height.as_u64(),
        });

        let json = serde_json::to_string_pretty(&output).unwrap();

        std::fs::write(out, json).expect("failed to write metadata");
    }

    Ok(())
}

/// The HTTP interface provides JSON-RPC request/response endpoints.
async fn run_http(mut provider: Provider<Http>, opts: &Options) -> anyhow::Result<()> {
    tracing::info!("Running the tests over HTTP...");
    adjust_provider(&mut provider);
    run(&provider, opts).await?;
    tracing::info!("HTTP tests finished");
    Ok(())
}
