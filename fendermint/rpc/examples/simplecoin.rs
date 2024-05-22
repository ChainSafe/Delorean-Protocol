// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Example of using the RPC library in combination with ethers abigen
//! to programmatically deploy and call a contract.
//!
//! The example assumes that Tendermint and Fendermint have been started
//! and are running locally.
//!
//! # Usage
//! ```text
//! cargo run -p fendermint_rpc --release --example simplecoin -- --secret-key test-network/keys/alice.sk --verbose
//! ```

use std::path::PathBuf;

use anyhow::{anyhow, Context};
use bytes::Bytes;
use clap::Parser;
use ethers::abi::Tokenizable;
use ethers::prelude::{abigen, decode_function_data};
use ethers::types::{H160, U256};
use fendermint_crypto::SecretKey;
use fendermint_rpc::query::QueryClient;
use fendermint_vm_actor_interface::eam::{self, CreateReturn, EthAddress};
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;
use fvm_shared::chainid::ChainID;
use lazy_static::lazy_static;
use tendermint_rpc::Url;
use tracing::Level;

use fvm_shared::econ::TokenAmount;

use fendermint_rpc::client::FendermintClient;
use fendermint_rpc::message::{GasParams, SignedMessageFactory};
use fendermint_rpc::tx::{CallClient, TxClient, TxCommit};

type MockProvider = ethers::providers::Provider<ethers::providers::MockProvider>;
type MockContractCall<T> = ethers::prelude::ContractCall<MockProvider, T>;

// Generate a statically typed interface for the contract.
// This assumes the `builtin-actors` repo is checked in next to Fendermint,
// which the `make actor-bundle` command takes care of if it wasn't.
// This path starts from the root of this project, not this file.
abigen!(SimpleCoin, "../testing/contracts/SimpleCoin.abi");

const CONTRACT_HEX: &'static str = include_str!("../../testing/contracts/SimpleCoin.bin");

lazy_static! {
    /// Default gas params based on the testkit.
    static ref GAS_PARAMS: GasParams = GasParams {
        gas_limit: 10_000_000_000,
        gas_fee_cap: TokenAmount::default(),
        gas_premium: TokenAmount::default(),
    };
}

// Alternatively we can generate the ABI code as follows:
// ```
//     ethers::prelude::Abigen::new("SimpleCoin", <path-to-abi>)
//         .unwrap()
//         .generate()
//         .unwrap()
//         .write_to_file("./simplecoin.rs")
//         .unwrap();
// ```
// This approach combined with `build.rs` was explored in https://github.com/filecoin-project/ref-fvm/pull/1507

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

    /// Path to the secret key to deploy with, expected to be in Base64 format.
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

    // Query the account nonce from the state, so it doesn't need to be passed as an arg.
    let sn = sequence(&client, &sk)
        .await
        .expect("error getting sequence");

    // Query the chain ID, so it doesn't need to be passed as an arg.
    // We could the chain name using `client.underlying().genesis().await?.chain_id.as_str()` as well.
    let chain_id = client
        .state_params(FvmQueryHeight::default())
        .await
        .expect("error getting state params")
        .value
        .chain_id;

    let mf = SignedMessageFactory::new_secp256k1(sk, sn, ChainID::from(chain_id));

    let mut client = client.bind(mf);

    run(&mut client).await.expect("failed to run example");
}

async fn run(
    client: &mut (impl TxClient<TxCommit> + QueryClient + CallClient),
) -> anyhow::Result<()> {
    let create_return = deploy_contract(client)
        .await
        .context("failed to deploy contract")?;
    let contract_addr = create_return.delegated_address();

    tracing::info!(
        contract_address = contract_addr.to_string(),
        actor_id = create_return.actor_id,
        "contract deployed"
    );

    let owner_addr = client.address();
    let owner_id = actor_id(client, &owner_addr)
        .await
        .context("failed to fetch owner ID")?;
    let owner_eth_addr = EthAddress::from_id(owner_id);

    let balance_call = get_balance(client, &create_return.eth_address, &owner_eth_addr, false)
        .await
        .context("failed to get balance with call")?;

    let balance_tx = get_balance(client, &create_return.eth_address, &owner_eth_addr, true)
        .await
        .context("failed to get balance with tx")?;

    assert_eq!(
        balance_call, balance_tx,
        "balance read with or without a transaction should be the same"
    );

    tracing::info!(
        balance = format!("{}", balance_call),
        owner_eth_addr = hex::encode(&owner_eth_addr.0),
        "owner balance"
    );

    let _sufficient = send_coin(client, &create_return.eth_address, &owner_eth_addr, 100)
        .await
        .context("failed to send coin")?;

    Ok(())
}

/// Get the next sequence number (nonce) of an account.
async fn sequence(client: &impl QueryClient, sk: &SecretKey) -> anyhow::Result<u64> {
    let pk = sk.public_key();
    let addr = Address::new_secp256k1(&pk.serialize()).unwrap();
    let state = client
        .actor_state(&addr, FvmQueryHeight::default())
        .await
        .context("failed to get actor state")?;

    match state.value {
        Some((_id, state)) => Ok(state.sequence),
        None => Err(anyhow!("cannot find actor {addr}")),
    }
}

async fn actor_id(client: &impl QueryClient, addr: &Address) -> anyhow::Result<u64> {
    let state = client
        .actor_state(addr, FvmQueryHeight::default())
        .await
        .context("failed to get actor state")?;

    match state.value {
        Some((id, _state)) => Ok(id),
        None => Err(anyhow!("cannot find actor {addr}")),
    }
}

/// Deploy SimpleCoin.
async fn deploy_contract(client: &mut impl TxClient<TxCommit>) -> anyhow::Result<CreateReturn> {
    let contract = hex::decode(&CONTRACT_HEX).context("error parsing contract")?;

    let res = client
        .fevm_create(
            Bytes::from(contract),
            Bytes::default(),
            TokenAmount::default(),
            GAS_PARAMS.clone(),
        )
        .await
        .context("error deploying contract")?;

    tracing::info!(tx_hash = ?res.response.hash, "deployment transaction");

    let ret = res.return_data.ok_or(anyhow!(
        "no CreateReturn data; response was {:?}",
        res.response
    ))?;

    Ok(ret)
}

/// Invoke or call SimpleCoin to query the balance of an account.
async fn get_balance(
    client: &mut (impl TxClient<TxCommit> + CallClient),
    contract_eth_addr: &EthAddress,
    owner_eth_addr: &EthAddress,
    in_transaction: bool,
) -> anyhow::Result<ethers::types::U256> {
    let contract = coin_contract(contract_eth_addr);
    let owner_h160_addr = eth_addr_to_h160(owner_eth_addr);
    let call = contract.get_balance(owner_h160_addr);

    let balance = invoke_or_call_contract(client, contract_eth_addr, call, in_transaction)
        .await
        .context("failed to call contract")?;

    Ok(balance)
}

/// Invoke or call SimpleCoin to send some coins to self.
async fn send_coin(
    client: &mut (impl TxClient<TxCommit> + CallClient),
    contract_eth_addr: &EthAddress,
    owner_eth_addr: &EthAddress,
    value: u32,
) -> anyhow::Result<bool> {
    let contract = coin_contract(contract_eth_addr);
    let owner_h160_addr = eth_addr_to_h160(owner_eth_addr);
    let call = contract.send_coin(owner_h160_addr, U256::from(value));

    let sufficient: bool = invoke_or_call_contract(client, contract_eth_addr, call, true)
        .await
        .context("failed to call contract")?;

    Ok(sufficient)
}

/// Invoke FEVM through Tendermint with the calldata encoded by ethers, decoding the result into the expected type.
async fn invoke_or_call_contract<T: Tokenizable>(
    client: &mut (impl TxClient<TxCommit> + CallClient),
    contract_eth_addr: &EthAddress,
    call: MockContractCall<T>,
    in_transaction: bool,
) -> anyhow::Result<T> {
    let calldata: ethers::types::Bytes = call
        .calldata()
        .expect("calldata should contain function and parameters");

    let contract_addr = eth_addr_to_eam(contract_eth_addr);

    // We can perform the read as a distributed transaction (if we don't trust any particular node to give the right answer),
    // or we can send a query with the same message and get a result without involving a transaction.
    let return_data = if in_transaction {
        let res = client
            .fevm_invoke(
                contract_addr,
                calldata.0,
                TokenAmount::default(),
                GAS_PARAMS.clone(),
            )
            .await
            .context("failed to invoke FEVM")?;

        tracing::info!(tx_hash = ?res.response.hash, "invoked transaction");

        res.return_data
    } else {
        let res = client
            .fevm_call(
                contract_addr,
                calldata.0,
                TokenAmount::default(),
                GAS_PARAMS.clone(),
                FvmQueryHeight::default(),
            )
            .await
            .context("failed to call FEVM")?;

        res.return_data
    };

    let bytes = return_data.ok_or(anyhow!("the contract did not return any data"))?;

    let res = decode_function_data(&call.function, bytes, false)
        .context("error deserializing return data")?;

    Ok(res)
}

/// Create an instance of the statically typed contract client.
fn coin_contract(contract_eth_addr: &EthAddress) -> SimpleCoin<MockProvider> {
    // A dummy client that we don't intend to use to call the contract or send transactions.
    let (client, _mock) = ethers::providers::Provider::mocked();
    let contract_h160_addr = eth_addr_to_h160(contract_eth_addr);
    let contract = SimpleCoin::new(contract_h160_addr, std::sync::Arc::new(client));
    contract
}

fn eth_addr_to_h160(eth_addr: &EthAddress) -> H160 {
    ethers::core::types::Address::from_slice(&eth_addr.0)
}

fn eth_addr_to_eam(eth_addr: &EthAddress) -> Address {
    Address::new_delegated(eam::EAM_ACTOR_ID, &eth_addr.0)
        .expect("ETH address to delegated should work")
}
