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
//! cargo run -p fendermint_eth_api --release --example ethers --
//! ```
//!
//! A method can also be called directly with `curl`:
//!
//! ```text
//! curl -X POST -i \
//!      -H 'Content-Type: application/json' \
//!      -d '{"jsonrpc":"2.0","id":0,"method":"eth_getBlockTransactionCountByNumber","params":["0x1"]}' \
//!      http://localhost:8545
//! ```

// See https://coinsbench.com/ethereum-with-rust-tutorial-part-1-create-simple-transactions-with-rust-26d365a7ea93
// and https://coinsbench.com/ethereum-with-rust-tutorial-part-2-compile-and-deploy-solidity-contract-with-rust-c3cd16fce8ee
// and https://coinsbench.com/ethers-rust-power-or-ethers-abigen-rundown-89ab5e47875d

use std::{fmt::Debug, path::PathBuf, sync::Arc};

use anyhow::{bail, Context};
use clap::Parser;
use common::{TestMiddleware, ENOUGH_GAS};
use ethers::providers::StreamExt;
use ethers::{
    prelude::{abigen, ContractFactory},
    providers::{FilterKind, Http, JsonRpcClient, Middleware, Provider, Ws},
    signers::Signer,
};
use ethers_core::{
    abi::Abi,
    types::{
        transaction::eip2718::TypedTransaction, Address, BlockId, BlockNumber, Bytes,
        Eip1559TransactionRequest, Filter, Log, SyncingStatus, TransactionReceipt, TxHash, H256,
        U256, U64,
    },
};
use tracing::Level;

use crate::common::{
    adjust_provider, make_middleware, prepare_call, request, send_transaction, TestAccount,
    TestContractCall,
};

mod common;

/// Disabling filters helps when inspecting docker logs. The background data received for filters is rather noisy.
const FILTERS_ENABLED: bool = true;

// Generate a statically typed interface for the contract.
// An example of what it looks like is at https://github.com/filecoin-project/ref-fvm/blob/evm-integration-tests/testing/integration/tests/evm/src/simple_coin/simple_coin.rs
abigen!(SimpleCoin, "../../testing/contracts/SimpleCoin.abi");

const SIMPLECOIN_HEX: &'static str = include_str!("../../../testing/contracts/SimpleCoin.bin");
const SIMPLECOIN_RUNTIME_HEX: &'static str =
    include_str!("../../../testing/contracts/SimpleCoin.bin-runtime");

#[derive(Parser, Debug)]
pub struct Options {
    /// The host of the Fendermint Ethereum API endpoint.
    #[arg(long, default_value = "127.0.0.1", env = "FM_ETH__LISTEN__HOST")]
    pub http_host: String,

    /// The port of the Fendermint Ethereum API endpoint.
    #[arg(long, default_value = "8545", env = "FM_ETH__LISTEN__PORT")]
    pub http_port: u32,

    /// Secret key used to send funds, expected to be in Base64 format.
    ///
    /// Assumed to exist with a non-zero balance.
    #[arg(long)]
    pub secret_key_from: PathBuf,

    /// Secret key used to receive funds, expected to be in Base64 format.
    #[arg(long)]
    pub secret_key_to: PathBuf,

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

    pub fn ws_endpoint(&self) -> String {
        // Same address but accessed with GET
        format!("ws://{}:{}", self.http_host, self.http_port)
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

    let provider = Provider::<Ws>::connect(opts.ws_endpoint()).await?;
    run_ws(provider, &opts).await?;

    Ok(())
}

// The following methods are called by the [`Provider`].
// This is not an exhaustive list of JSON-RPC methods that the API implements, just what the client library calls.
//
// DONE:
// - eth_accounts
// - eth_blockNumber
// - eth_chainId
// - eth_getBalance
// - eth_getUncleCountByBlockHash
// - eth_getUncleCountByBlockNumber
// - eth_getUncleByBlockHashAndIndex
// - eth_getUncleByBlockNumberAndIndex
// - eth_getTransactionCount
// - eth_gasPrice
// - eth_getBlockByHash
// - eth_getBlockByNumber
// - eth_getTransactionByHash
// - eth_getTransactionReceipt
// - eth_feeHistory
// - eth_maxPriorityFeePerGas
// - eth_sendRawTransaction
// - eth_call
// - eth_estimateGas
// - eth_getBlockReceipts
// - eth_getStorageAt
// - eth_getCode
// - eth_syncing
// - web3_clientVersion
// - eth_getLogs
// - eth_newFilter
// - eth_newBlockFilter
// - eth_newPendingTransactionFilter
// - eth_getFilterChanges
// - eth_uninstallFilter
// - eth_subscribe
// - eth_unsubscribe
//
// DOING:
//
// TODO:
//
// WON'T DO:
// - eth_sign
// - eth_sendTransaction
// - eth_mining
// - eth_createAccessList
// - eth_getProof
//

/// Exercise the above methods, so we know at least the parameters are lined up correctly.
async fn run<C>(provider: &Provider<C>, opts: &Options) -> anyhow::Result<()>
where
    C: JsonRpcClient + Clone + 'static,
{
    let from = TestAccount::new(&opts.secret_key_from)?;
    let to = TestAccount::new(&opts.secret_key_to)?;

    tracing::info!(from = ?from.eth_addr, to = ?to.eth_addr, "ethereum address");

    // Set up filters to collect events.
    let mut filter_ids = Vec::new();

    let (logs_filter_id, blocks_filter_id, txs_filter_id): (
        Option<U256>,
        Option<U256>,
        Option<U256>,
    ) = if FILTERS_ENABLED {
        let logs_filter_id = request(
            "eth_newFilter",
            provider
                .new_filter(FilterKind::Logs(&Filter::default()))
                .await,
            |_| true,
        )?;
        filter_ids.push(logs_filter_id);

        let blocks_filter_id = request(
            "eth_newBlockFilter",
            provider.new_filter(FilterKind::NewBlocks).await,
            |id| *id != logs_filter_id,
        )?;
        filter_ids.push(blocks_filter_id);

        let txs_filter_id = request(
            "eth_newPendingTransactionFilter",
            provider.new_filter(FilterKind::PendingTransactions).await,
            |id| *id != logs_filter_id,
        )?;
        filter_ids.push(txs_filter_id);

        (
            Some(logs_filter_id),
            Some(blocks_filter_id),
            Some(txs_filter_id),
        )
    } else {
        (None, None, None)
    };

    request("web3_clientVersion", provider.client_version().await, |v| {
        v.starts_with("fendermint/")
    })?;

    request("net_version", provider.get_net_version().await, |v| {
        !v.is_empty() && v.chars().all(|c| c.is_numeric())
    })?;

    request("eth_accounts", provider.get_accounts().await, |acnts| {
        acnts.is_empty()
    })?;

    let bn = request("eth_blockNumber", provider.get_block_number().await, |bn| {
        bn.as_u64() > 0
    })?;

    // Go back one block, so we can be sure there are results.
    let bn = bn - 1;

    let chain_id = request("eth_chainId", provider.get_chainid().await, |id| {
        !id.is_zero()
    })?;

    let mw = make_middleware(provider.clone(), chain_id.as_u64(), &from)
        .context("failed to create middleware")?;
    let mw = Arc::new(mw);

    request(
        "eth_getBalance",
        provider.get_balance(from.eth_addr, None).await,
        |b| !b.is_zero(),
    )?;

    request(
        "eth_getBalance (non-existent)",
        provider.get_balance(Address::default(), None).await,
        |b| b.is_zero(),
    )?;

    request(
        "eth_getUncleCountByBlockHash",
        provider
            .get_uncle_count(BlockId::Hash(H256([0u8; 32])))
            .await,
        |uc| uc.is_zero(),
    )?;

    request(
        "eth_getUncleCountByBlockNumber",
        provider
            .get_uncle_count(BlockId::Number(BlockNumber::Number(bn)))
            .await,
        |uc| uc.is_zero(),
    )?;

    request(
        "eth_getUncleByBlockHashAndIndex",
        provider
            .get_uncle(BlockId::Hash(H256([0u8; 32])), U64::from(0))
            .await,
        |u| u.is_none(),
    )?;

    request(
        "eth_getUncleByBlockNumberAndIndex",
        provider
            .get_uncle(BlockId::Number(BlockNumber::Number(bn)), U64::from(0))
            .await,
        |u| u.is_none(),
    )?;

    // Get a block without transactions
    let b = request(
        "eth_getBlockByNumber w/o txns",
        provider
            .get_block(BlockId::Number(BlockNumber::Number(bn)))
            .await,
        |b| b.is_some() && b.as_ref().map(|b| b.number).flatten() == Some(bn),
    )?;

    let bh = b.unwrap().hash.expect("hash should be set");

    // Get the same block without transactions by hash.
    request(
        "eth_getBlockByHash w/o txns",
        provider.get_block(BlockId::Hash(bh)).await,
        |b| b.is_some() && b.as_ref().map(|b| b.number).flatten() == Some(bn),
    )?;

    // Get the synthetic zero block.
    let b = request(
        "eth_getBlockByNumber @ zero",
        provider
            .get_block(BlockId::Number(BlockNumber::Number(U64::from(0))))
            .await,
        |b| b.is_some(),
    )?;

    let bh = b.unwrap().hash.expect("hash should be set");

    // Check that block 0 can be fetched by its hash.
    request(
        "eth_getBlockByHash @ zero",
        provider.get_block(BlockId::Hash(bh)).await,
        |b| b.is_some() && b.as_ref().map(|b| b.number).flatten() == Some(U64::from(0)),
    )?;

    // Check that block 1 points at the synthetic block 0 as parent.
    request(
        "eth_getBlockByNumber @ one",
        provider
            .get_block(BlockId::Number(BlockNumber::Number(U64::from(1))))
            .await,
        |b| b.is_some() && b.as_ref().map(|b| b.parent_hash) == Some(bh),
    )?;

    let base_fee = request("eth_gasPrice", provider.get_gas_price().await, |id| {
        !id.is_zero()
    })?;

    tracing::info!("sending example transfer");

    let transfer = make_transfer(&mw, &to)
        .await
        .context("failed to make a transfer")?;

    let receipt = send_transaction(&mw, transfer.clone(), "transfer")
        .await
        .context("failed to send transfer")?;

    let tx_hash = receipt.transaction_hash;
    let bn = receipt.block_number.unwrap();
    let bh = receipt.block_hash.unwrap();

    tracing::info!(height = ?bn, ?tx_hash, "example transfer");

    // This equivalence is not required for ethers-rs, it's happy to use the return value from `eth_sendRawTransaction` for transaction hash.
    // However, ethers.js actually asserts this and we cannot disable it, rendering that, or any similar tool, unusable if we rely on
    // the default Tendermint transaction hash, which is a Sha256 hash of the entire payload (which includes the signature),
    // not a Keccak256 of the unsigned RLP.

    let expected_hash = {
        let sig = mw
            .signer()
            .sign_transaction(&transfer)
            .await
            .context("failed to sign transaction")?;

        let rlp = transfer.rlp_signed(&sig);
        TxHash::from(ethers_core::utils::keccak256(rlp))
    };
    assert_eq!(tx_hash, expected_hash, "Ethereum hash should match");

    // Querying at latest, so the transaction count should be non-zero.
    request(
        "eth_getTransactionCount",
        provider.get_transaction_count(from.eth_addr, None).await,
        |u| !u.is_zero(),
    )?;

    request(
        "eth_getTransactionCount (non-existent)",
        provider
            .get_transaction_count(Address::default(), None)
            .await,
        |b| b.is_zero(),
    )?;

    // Get a block with transactions by number.
    let block = request(
        "eth_getBlockByNumber w/ txns",
        provider
            .get_block_with_txs(BlockId::Number(BlockNumber::Number(bn)))
            .await,
        |b| b.is_some() && b.as_ref().map(|b| b.number).flatten() == Some(bn),
    )?;

    assert_eq!(
        tx_hash,
        block.unwrap().transactions[0].hash,
        "computed hash should match"
    );

    // Get the block with transactions by hash.
    request(
        "eth_getBlockByHash w/ txns",
        provider.get_block_with_txs(BlockId::Hash(bh)).await,
        |b| b.is_some() && b.as_ref().map(|b| b.number).flatten() == Some(bn),
    )?;

    // By now there should be a transaction in a block.
    request(
        "eth_feeHistory",
        provider
            .fee_history(
                U256::from(100),
                BlockNumber::Latest,
                &[0.25, 0.5, 0.75, 0.95],
            )
            .await,
        |hist| {
            hist.base_fee_per_gas.len() > 0
                && *hist.base_fee_per_gas.last().unwrap() == base_fee
                && hist.gas_used_ratio.iter().any(|r| *r > 0.0)
        },
    )?;

    request(
        "eth_getTransactionByHash",
        provider.get_transaction(tx_hash).await,
        |tx| tx.is_some(),
    )?;

    request(
        "eth_getTransactionReceipt",
        provider.get_transaction_receipt(tx_hash).await,
        |tx| tx.is_some(),
    )?;

    request(
        "eth_getBlockReceipts",
        provider.get_block_receipts(BlockNumber::Number(bn)).await,
        |rs| !rs.is_empty(),
    )?;

    // Calling with 0 nonce so the node figures out the latest value.
    let mut probe_tx = transfer.clone();
    probe_tx.set_nonce(0);

    let probe_height = BlockId::Number(BlockNumber::Number(bn));

    request(
        "eth_call",
        provider.call(&probe_tx, Some(probe_height)).await,
        |_| true,
    )?;

    request(
        "eth_estimateGas w/ height",
        provider.estimate_gas(&probe_tx, Some(probe_height)).await,
        |gas: &U256| !gas.is_zero(),
    )?;

    request(
        "eth_estimateGas w/o height",
        provider.estimate_gas(&probe_tx, None).await,
        |gas: &U256| !gas.is_zero(),
    )?;

    request(
        "eth_maxPriorityFeePerGas",
        provider.request("eth_maxPriorityFeePerGas", ()).await,
        |premium: &U256| !premium.is_zero(),
    )?;

    tracing::info!("deploying SimpleCoin");

    let bytecode =
        Bytes::from(hex::decode(SIMPLECOIN_HEX).context("failed to decode contract hex")?);

    let deployed_bytecode = Bytes::from(
        hex::decode(SIMPLECOIN_RUNTIME_HEX).context("failed to decode contract runtime hex")?,
    );

    // let abi = serde_json::from_str::<ethers::core::abi::Abi>(SIMPLECOIN_ABI)?;
    let abi: Abi = SIMPLECOIN_ABI.clone();

    let factory = ContractFactory::new(abi, bytecode.clone(), mw.clone());
    let mut deployer = factory.deploy(())?;

    // Fill the fields so we can debug any difference between this and the node.
    // Using `Some` block ID because with `None` the eth_estimateGas call would receive invalid parameters.
    mw.fill_transaction(&mut deployer.tx, Some(BlockId::Number(BlockNumber::Latest)))
        .await
        .context("failed to fill deploy transaction")?;

    tracing::info!(sighash = ?deployer.tx.sighash(), "deployment tx");

    // Try with a call just because Remix does.
    request(
        "eth_call w/ deploy",
        provider.call(&deployer.tx, None).await,
        |_| true,
    )?;

    // NOTE: This would call eth_estimateGas to figure out how much gas to use, if we didn't set it.
    // What the [Provider::fill_transaction] will _also_ do is estimate the fees using eth_feeHistory, here:
    // https://github.com/gakonst/ethers-rs/blob/df165b84229cdc1c65e8522e0c1aeead3746d9a8/ethers-providers/src/rpc/provider.rs#LL300C30-L300C51
    // These were set to zero in the earlier example transfer, ie. it was basically paid for by the miner (which is not at the moment charged),
    // so the test passed. Here, however, there will be a non-zero cost to pay by the deployer, and therefore those balances
    // have to be much higher than the defaults used earlier, e.g. the deployment cost 30 FIL, and we used to give 1 FIL.
    let (contract, deploy_receipt): (_, TransactionReceipt) = deployer
        .send_with_receipt()
        .await
        .context("failed to send deployment")?;

    tracing::info!(addr = ?contract.address(), "SimpleCoin deployed");

    let contract = SimpleCoin::new(contract.address(), contract.client());

    let coin_balance: TestContractCall<_, U256> =
        prepare_call(&mw, contract.get_balance(from.eth_addr), false).await?;

    request("eth_call", coin_balance.call().await, |coin_balance| {
        *coin_balance == U256::from(10000)
    })?;

    // Calling with 0x00..00 address so we see if it world work for calls by clients that set nothing.
    let coin_balance = coin_balance.from(Address::default());

    request(
        "eth_call w/ 0x00..00",
        coin_balance.call().await,
        |coin_balance| *coin_balance == U256::from(10000),
    )?;

    // Call a method that does a revert, to check that the message shows up in the return value.
    // Try to send more than the available balance of 10,000
    let coin_send: TestContractCall<_, ()> = prepare_call(
        &mw,
        contract.send_coin_or_revert(to.eth_addr, U256::from(10000 * 10)),
        true,
    )
    .await
    .context("failed to prepare revert call")?;

    match coin_send.call().await {
        Ok(_) => bail!("call should failed with a revert"),
        Err(e) => {
            let e = e.to_string();
            assert!(e.contains("revert"), "should say revert");
            assert!(e.contains("0x08c379a"), "should have string selector");
        }
    }

    // We could calculate the storage location of the balance of the owner of the contract,
    // but let's just see what it returns with at slot 0. See an example at
    // https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_getstorageat
    let storage_location = {
        let mut bz = [0u8; 32];
        U256::zero().to_big_endian(&mut bz);
        H256::from_slice(&bz)
    };

    request(
        "eth_getStorageAt",
        mw.get_storage_at(contract.address(), storage_location, None)
            .await,
        |_| true,
    )?;

    request(
        "eth_getStorageAt /w account",
        mw.get_storage_at(from.eth_addr, storage_location, None)
            .await,
        |_| true,
    )?;

    request(
        "eth_getCode",
        mw.get_code(contract.address(), None).await,
        |bz| *bz == deployed_bytecode,
    )?;

    request(
        "eth_getCode /w account",
        mw.get_code(from.eth_addr, None).await,
        |bz| bz.is_empty(),
    )?;

    request("eth_syncing", mw.syncing().await, |s| {
        *s == SyncingStatus::IsFalse // There is only one node.
    })?;

    // Send a SimpleCoin transaction to get an event emitted.
    // Not using `prepare_call` here because `send_transaction` will fill the missing fields.
    let coin_send_value = U256::from(100);
    let coin_send: TestContractCall<_, bool> = contract.send_coin(to.eth_addr, coin_send_value);

    // Take note of the inputs to ascertain it's the same we get back.
    let tx_input = match coin_send.tx {
        TypedTransaction::Eip1559(ref tx) => tx.data.clone(),
        _ => None,
    };

    // Using `send_transaction` instead of `coin_send.send()` so it gets the receipt.
    // Unfortunately the returned `bool` is not available through the Ethereum API.
    let receipt = request(
        "eth_sendRawTransaction",
        send_transaction(&mw, coin_send.tx, "coin_send").await,
        |receipt| !receipt.logs.is_empty() && receipt.logs.iter().all(|l| l.log_type.is_none()),
    )?;

    tracing::info!(tx_hash = ?receipt.transaction_hash, "coin sent");

    request(
        "eth_getTransactionByHash for input",
        provider.get_transaction(receipt.transaction_hash).await,
        |tx| match tx {
            Some(tx) => tx.input == tx_input.unwrap_or_default(),
            _ => false,
        },
    )?;

    request(
        "eth_getLogs",
        mw.get_logs(&Filter::new().at_block_hash(receipt.block_hash.unwrap()))
            .await,
        |logs| *logs == receipt.logs,
    )?;

    // Check that requesting logs with higher-than-highest height does not fail.
    request(
        "eth_getLogs /w too high 'to' height",
        mw.get_logs(&Filter::new().to_block(BlockNumber::Number(U64::from(u32::MAX))))
            .await,
        |logs: &Vec<Log>| logs.is_empty(), // There will be nothing from latest-to-latest by now.
    )?;

    // See what kind of events were logged.

    if let Some(blocks_filter_id) = blocks_filter_id {
        request(
            "eth_getFilterChanges (blocks)",
            mw.get_filter_changes(blocks_filter_id).await,
            |block_hashes: &Vec<H256>| {
                [bh, deploy_receipt.block_hash.unwrap()]
                    .iter()
                    .all(|h| block_hashes.contains(h))
            },
        )?;
    }

    if let Some(txs_filter_id) = txs_filter_id {
        request(
            "eth_getFilterChanges (txs)",
            mw.get_filter_changes(txs_filter_id).await,
            |tx_hashes: &Vec<H256>| {
                [&tx_hash, &deploy_receipt.transaction_hash]
                    .iter()
                    .all(|h| tx_hashes.contains(h))
            },
        )?;
    }

    if let Some(logs_filter_id) = logs_filter_id {
        let logs = request(
            "eth_getFilterChanges (logs)",
            mw.get_filter_changes(logs_filter_id).await,
            |logs: &Vec<Log>| !logs.is_empty(),
        )?;

        // eprintln!("LOGS = {logs:?}");

        // Parse `Transfer` events from the logs with the SimpleCoin contract.
        // Based on https://github.com/filecoin-project/ref-fvm/blob/evm-integration-tests/testing/integration/tests/fevm_features/common.rs#L616
        //      and https://github.com/filecoin-project/ref-fvm/blob/evm-integration-tests/testing/integration/tests/fevm_features/simple_coin.rs#L26
        //      and https://github.com/filecoin-project/ref-fvm/blob/evm-integration-tests/testing/integration/tests/evm/src/simple_coin/simple_coin.rs#L103

        // The contract has methods like `.transfer_filter()` which allows querying logs, but here we just test parsing to make sure the data is correct.
        let transfer_events = logs
            .into_iter()
            .filter(|log| log.address == contract.address())
            .map(|log| contract.decode_event::<TransferFilter>("Transfer", log.topics, log.data))
            .collect::<Result<Vec<_>, _>>()
            .context("failed to parse logs to transfer events")?;

        assert!(!transfer_events.is_empty());
        assert_eq!(transfer_events[0].from, from.eth_addr);
        assert_eq!(transfer_events[0].to, to.eth_addr);
        assert_eq!(transfer_events[0].value, coin_send_value);
    }

    // Uninstall all filters.
    for id in filter_ids {
        request("eth_uninstallFilter", mw.uninstall_filter(id).await, |ok| {
            *ok
        })?;
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

/// The WebSocket interface provides JSON-RPC request/response interactions
/// as well as subscriptions, both using messages over the socket.
///
/// We subscribe to notifications first, then run the same suite of request/responses
/// as the HTTP case, finally check that we have collected events over the subscriptions.
async fn run_ws(mut provider: Provider<Ws>, opts: &Options) -> anyhow::Result<()> {
    tracing::info!("Running the tests over WS...");
    adjust_provider(&mut provider);

    // Subscriptions as well.
    let subs = if FILTERS_ENABLED {
        let block_sub = provider.subscribe_blocks().await?;
        let txs_sub = provider.subscribe_pending_txs().await?;
        let log_sub = provider.subscribe_logs(&Filter::default()).await?;
        Some((block_sub, txs_sub, log_sub))
    } else {
        None
    };

    run(&provider, opts).await?;

    if let Some((mut block_sub, mut txs_sub, mut log_sub)) = subs {
        assert!(block_sub.next().await.is_some(), "blocks should arrive");
        assert!(txs_sub.next().await.is_some(), "transactions should arrive");
        assert!(log_sub.next().await.is_some(), "logs should arrive");

        block_sub
            .unsubscribe()
            .await
            .context("failed to unsubscribe blocks")?;

        txs_sub
            .unsubscribe()
            .await
            .context("failed to unsubscribe txs")?;

        log_sub
            .unsubscribe()
            .await
            .context("failed to unsubscribe logs")?;
    }

    tracing::info!("WS tests finished.");
    Ok(())
}

async fn make_transfer<C>(
    mw: &TestMiddleware<C>,
    to: &TestAccount,
) -> anyhow::Result<TypedTransaction>
where
    C: JsonRpcClient + 'static,
{
    // Create a transaction to transfer 1000 atto.
    let tx = Eip1559TransactionRequest::new().to(to.eth_addr).value(1000);

    // Set the gas based on the testkit so it doesn't trigger estimation.
    let mut tx = tx
        .gas(ENOUGH_GAS)
        .max_fee_per_gas(0)
        .max_priority_fee_per_gas(0)
        .into();

    // Fill in the missing fields like `from` and `nonce` (which involves querying the API).
    mw.fill_transaction(&mut tx, None).await?;

    Ok(tx)
}
