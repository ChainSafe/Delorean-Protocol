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
//! cargo run -p fendermint_eth_api --release --example query_blockhash --
//! ```

use anyhow::Context;
use clap::Parser;
use ethers::{
    prelude::{abigen, ContractFactory},
    providers::{Http, JsonRpcClient, Middleware, Provider},
};
use ethers_core::{
    abi::Abi,
    types::{BlockId, BlockNumber, Bytes, TransactionReceipt, H256, U256, U64},
};
use hex;
use std::{fmt::Debug, path::PathBuf, sync::Arc};
use tracing::Level;

use crate::common::{adjust_provider, make_middleware, request, TestAccount};

#[allow(dead_code)]
mod common;

// Generate a statically typed interface for the contract.
abigen!(QueryBlockhash, "../../testing/contracts/QueryBlockhash.abi");

const QUERYBLOCKHASH_HEX: &'static str =
    include_str!("../../../testing/contracts/QueryBlockhash.bin");

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
    tracing::info!("deploying QueryBlockhash");

    let bytecode =
        Bytes::from(hex::decode(QUERYBLOCKHASH_HEX).context("failed to decode contract hex")?);
    let abi: Abi = QUERYBLOCKHASH_ABI.clone();

    let chain_id = provider.get_chainid().await?;

    let mw = make_middleware(provider.clone(), chain_id.as_u64(), &from)
        .context("failed to create middleware")?;

    let mw = Arc::new(mw);

    let factory = ContractFactory::new(abi, bytecode.clone(), mw.clone());
    let deployer = factory.deploy(())?;

    let (contract, deploy_receipt): (_, TransactionReceipt) = deployer
        .send_with_receipt()
        .await
        .context("failed to send deployment")?;

    tracing::info!(addr = ?contract.address(), "QueryBlockhash deployed");

    let contract = QueryBlockhash::new(contract.address(), contract.client());

    // check the deploy_height so we don't risk asking for blocks that had
    // been removed from the chainmetadata state (it has a relatively short
    // lookback length of 256)
    let deploy_height = deploy_receipt
        .block_number
        .expect("deploy height should be set")
        .as_u64();
    tracing::info!("deploy_height: {:?}", deploy_height);

    // we want check the get the blockhash for the last 5 blocks
    const NR_CHECKS: u64 = 5;
    let start_block = if deploy_height >= NR_CHECKS {
        deploy_height - NR_CHECKS
    } else {
        0
    };

    // check that the blockhash returned by the contract matches the one returned by tendermint
    for epoch in start_block..deploy_height {
        tracing::info!("Checking blockhashes at epoch: {}", epoch);

        // get the blockhash from the contract, which results in call to get_tipset_cid in fendermint
        //
        let blockhash: [u8; 32] = contract
            .get_blockhash(U256::from(epoch))
            .call()
            .await
            .context("failed to call get_blockhash")?;
        let blockhash = H256::from_slice(&blockhash);
        tracing::info!("blockhash from contract: {:?}", blockhash);

        // get the blockhash from tendermint
        //
        let b = request(
            "eth_getBlockByNumber w/o txns",
            provider
                .get_block(BlockId::Number(BlockNumber::Number(U64::from(epoch))))
                .await,
            |b| b.is_some() && b.as_ref().map(|b| b.number).flatten() == Some(U64::from(epoch)),
        )?;
        let bh = b.unwrap().hash.expect("hash should be set");
        tracing::info!("blockhash from API:      {:?}", bh);

        assert_eq!(blockhash, bh);
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
