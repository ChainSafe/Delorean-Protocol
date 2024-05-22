// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use ethers::{
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{JsonRpcClient, Middleware, PendingTransaction, Provider},
    signers::{Signer, Wallet},
    types::{transaction::eip2718::TypedTransaction, Eip1559TransactionRequest, H160},
};
use fendermint_materializer::{manifest::Rootnet, materials::DefaultAccount, HasEthApi};
use futures::FutureExt;

use crate::with_testnet;

const MANIFEST: &str = "standalone.yaml";

pub type TestMiddleware<C> = SignerMiddleware<Provider<C>, Wallet<SigningKey>>;

/// Create a middleware that will assign nonces and sign the message.
async fn make_middleware<C>(
    provider: Provider<C>,
    sender: &DefaultAccount,
) -> anyhow::Result<TestMiddleware<C>>
where
    C: JsonRpcClient,
{
    let chain_id = provider
        .get_chainid()
        .await
        .context("failed to get chain ID")?;

    let wallet: Wallet<SigningKey> = Wallet::from_bytes(sender.secret_key().serialize().as_ref())?
        .with_chain_id(chain_id.as_u64());

    Ok(SignerMiddleware::new(provider, wallet))
}

/// Test that a transaction sent to the mempool can be retrieved by its ethereum hash
/// from the ethereum API instance it was sent to even before it is included in the block.
#[serial_test::serial]
#[tokio::test]
async fn test_sent_tx_found_in_mempool() {
    with_testnet(
        MANIFEST,
        |manifest| {
            // Slow down consensus to where we can see the effect of the transaction not being found by Ethereum hash.
            if let Rootnet::New { ref mut env, .. } = manifest.rootnet {
                env.insert("CMT_CONSENSUS_TIMEOUT_COMMIT".into(), "10s".into());
            };
        },
        |_, _, testnet| {
            let test = async {
                let bob = testnet.account("bob")?;
                let charlie = testnet.account("charlie")?;

                let pangea = testnet.node(&testnet.root().node("pangea"))?;
                let provider = pangea
                    .ethapi_http_provider()?
                    .expect("ethapi should be enabled");

                let middleware = make_middleware(provider, bob)
                    .await
                    .context("failed to set up middleware")?;

                // Create the simplest transaction possible: send tokens between accounts.
                let to: H160 = charlie.eth_addr().into();
                let transfer = Eip1559TransactionRequest::new().to(to).value(1);

                let pending: PendingTransaction<_> = middleware
                    .send_transaction(transfer, None)
                    .await
                    .context("failed to send txn")?;

                let tx_hash = pending.tx_hash();

                // We expect that the transaction is pending, however it should not return an error.
                match middleware.get_transaction(tx_hash).await {
                    Ok(Some(_)) => {}
                    Ok(None) => bail!("pending transaction not found by eth hash"),
                    Err(e) => {
                        bail!("failed to get pending transaction: {e}")
                    }
                }

                Ok(())
            };

            test.boxed_local()
        },
    )
    .await
    .unwrap()
}

/// Test that transactions sent out-of-order with regards to the nonce are not rejected,
/// but rather get included in block eventually, their submission managed by the ethereum
/// API facade.
#[serial_test::serial]
#[tokio::test]
async fn test_out_of_order_mempool() {
    const MAX_WAIT_TIME: Duration = Duration::from_secs(10);
    const SLEEP_TIME: Duration = Duration::from_secs(1);

    with_testnet(
        MANIFEST,
        |_| {},
        |_, _, testnet| {
            let test = async {
                let bob = testnet.account("bob")?;
                let charlie = testnet.account("charlie")?;

                let pangea = testnet.node(&testnet.root().node("pangea"))?;
                let provider = pangea
                    .ethapi_http_provider()?
                    .expect("ethapi should be enabled");

                let middleware = make_middleware(provider, bob)
                    .await
                    .context("failed to set up middleware")?;

                // Create the simplest transaction possible: send tokens between accounts.
                let to: H160 = charlie.eth_addr().into();
                let tx = Eip1559TransactionRequest::new().to(to).value(1);
                let mut tx: TypedTransaction = tx.into();

                // Fill out the nonce, gas, etc.
                middleware
                    .fill_transaction(&mut tx, None)
                    .await
                    .context("failed to fill tx")?;

                // Create a few more transactions to be sent out-of-order.
                let mut txs = vec![tx];

                for i in 1..5 {
                    let mut tx = txs[0].clone();
                    let nonce = tx.nonce().expect("fill_transaction filled the nonce");
                    tx.set_nonce(nonce.saturating_add(i.into()));
                    txs.push(tx)
                }

                let mut pending_txs = Vec::new();

                // Submit transactions in opposite order.
                for (i, tx) in txs.iter().enumerate().rev() {
                    let sig = middleware
                        .signer()
                        .sign_transaction(tx)
                        .await
                        .context("failed to sign tx")?;

                    let rlp = tx.rlp_signed(&sig);

                    let pending_tx: PendingTransaction<_> = middleware
                        .send_raw_transaction(rlp)
                        .await
                        .with_context(|| format!("failed to send tx {i}"))?;

                    pending_txs.push(pending_tx)
                }

                // Check that they eventually get included.
                let start = Instant::now();
                'pending: loop {
                    for tx in pending_txs.iter() {
                        let receipt = middleware
                            .get_transaction_receipt(tx.tx_hash())
                            .await
                            .context("failed to get receipt")?;

                        if receipt.is_none() {
                            if start.elapsed() > MAX_WAIT_TIME {
                                bail!("some transactions are still not executed")
                            } else {
                                tokio::time::sleep(SLEEP_TIME).await;
                                continue 'pending;
                            }
                        }
                    }
                    // All of them have receipt.
                    break 'pending;
                }

                Ok(())
            };

            test.boxed_local()
        },
    )
    .await
    .unwrap()
}
