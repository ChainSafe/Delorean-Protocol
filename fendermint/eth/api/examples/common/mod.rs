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

use std::{fmt::Debug, fmt::Display};
use std::{path::Path, time::Duration};

use anyhow::{anyhow, Context};
use ethers::{
    prelude::{ContractCall, SignerMiddleware},
    providers::{JsonRpcClient, Middleware, Provider},
    signers::{Signer, Wallet},
};
use ethers_core::{
    k256::ecdsa::SigningKey,
    types::{
        transaction::eip2718::TypedTransaction, Address, BlockId, BlockNumber, TransactionReceipt,
        H160,
    },
};
use fendermint_crypto::SecretKey;
use fendermint_rpc::message::SignedMessageFactory;
use fendermint_vm_actor_interface::eam::EthAddress;

pub type TestMiddleware<C> = SignerMiddleware<Provider<C>, Wallet<SigningKey>>;
pub type TestContractCall<C, T> = ContractCall<TestMiddleware<C>, T>;

/// Gas limit to set for transactions.
pub const ENOUGH_GAS: u64 = 10_000_000_000u64;

pub struct TestAccount {
    pub secret_key: SecretKey,
    pub eth_addr: H160,
}

impl TestAccount {
    pub fn new(sk: &Path) -> anyhow::Result<Self> {
        let sk = SignedMessageFactory::read_secret_key(sk)?;
        let ea = EthAddress::from(sk.public_key());
        let h = Address::from_slice(&ea.0);

        Ok(Self {
            secret_key: sk,
            eth_addr: h,
        })
    }
}

pub fn adjust_provider<C>(provider: &mut Provider<C>)
where
    C: JsonRpcClient,
{
    // Tendermint block interval is lower.
    provider.set_interval(Duration::from_secs(2));
}

/// Send a transaction and await the receipt.
pub async fn send_transaction<C>(
    mw: &TestMiddleware<C>,
    tx: TypedTransaction,
    label: &str,
) -> anyhow::Result<TransactionReceipt>
where
    C: JsonRpcClient + 'static,
{
    // `send_transaction` will fill in the missing fields like `from` and `nonce` (which involves querying the API).
    let receipt = mw
        .send_transaction(tx, None)
        .await
        .context("failed to send transaction")?
        .log_msg(format!("Pending transaction: {label}"))
        .retries(5)
        .await?
        .context("Missing receipt")?;

    Ok(receipt)
}

/// Create a middleware that will assign nonces and sign the message.
pub fn make_middleware<C>(
    provider: Provider<C>,
    chain_id: u64,
    sender: &TestAccount,
) -> anyhow::Result<TestMiddleware<C>>
where
    C: JsonRpcClient,
{
    // We have to use Ethereum's signing scheme, beause the `from` is not part of the RLP representation,
    // it is inferred from the public key recovered from the signature. We could potentially hash the
    // transaction in a different way, but we can't for example use the actor ID in the hash, because
    // we have no way of sending it along with the message.
    let wallet: Wallet<SigningKey> =
        Wallet::from_bytes(&sender.secret_key.serialize().as_ref())?.with_chain_id(chain_id);

    Ok(SignerMiddleware::new(provider, wallet))
}

/// Fill the transaction fields such as gas and nonce.
pub async fn prepare_call<C, T>(
    mw: &TestMiddleware<C>,
    mut call: TestContractCall<C, T>,
    prevent_estimation: bool,
) -> anyhow::Result<TestContractCall<C, T>>
where
    C: JsonRpcClient + 'static,
{
    if prevent_estimation {
        // Set the gas based on the testkit so it doesn't trigger estimation.
        let tx = call.tx.as_eip1559_mut();
        let tx = tx.expect("eip1559");

        tx.gas = Some(ENOUGH_GAS.into());
        tx.max_fee_per_gas = Some(0.into());
        tx.max_priority_fee_per_gas = Some(0.into());
    }

    // Fill in the missing fields like `from` and `nonce` (which involves querying the API).
    mw.fill_transaction(&mut call.tx, Some(BlockId::Number(BlockNumber::Latest)))
        .await
        .context("failed to fill transaction")?;

    Ok(call)
}

pub trait CheckResult {
    fn check_result(&self) -> anyhow::Result<()>;
}

impl CheckResult for bool {
    fn check_result(&self) -> anyhow::Result<()> {
        if *self {
            Ok(())
        } else {
            Err(anyhow!("expected true; got false"))
        }
    }
}

pub fn request<T, E, F, C>(method: &str, res: Result<T, E>, check: F) -> anyhow::Result<T>
where
    T: Debug,
    F: FnOnce(&T) -> C,
    C: CheckResult,
    E: Display,
{
    tracing::debug!("checking request {method}...");
    match res {
        Ok(value) => match check(&value).check_result() {
            Ok(()) => Ok(value),
            Err(e) => Err(anyhow!("failed to check {method}: {e}:\n{value:?}")),
        },
        Err(e) => Err(anyhow!("failed to call {method}: {e:#}")),
    }
}
