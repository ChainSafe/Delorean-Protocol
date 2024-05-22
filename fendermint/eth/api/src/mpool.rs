// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Utilities related to caching and buffering Ethereum transactions.
use std::{collections::BTreeMap, time::Duration};

use ethers_core::types as et;
use fendermint_rpc::{
    client::TendermintClient, message::SignedMessageFactory, FendermintClient, QueryClient,
};
use fendermint_vm_message::{chain::ChainMessage, query::FvmQueryHeight, signed::DomainHash};
use futures::StreamExt;
use fvm_shared::{address::Address, chainid::ChainID};
use tendermint::Block;
use tendermint_rpc::{
    event::EventData,
    query::{EventType, Query},
    Client, SubscriptionClient,
};

use crate::{cache::Cache, state::Nonce, HybridClient};

const RETRY_SLEEP_SECS: u64 = 5;

/// Cache submitted transactions by their Ethereum hash, because the CometBFT
/// API would not be able to find them until they are delivered to the application
/// and indexed by their domain hash, which some tools interpret as the transaction
/// being dropped from the mempool.
pub type TransactionCache = Cache<et::TxHash, et::Transaction>;

/// Buffer out-of-order messages until they can be sent to the chain.
#[derive(Clone)]
pub struct TransactionBuffer(pub Cache<Address, BTreeMap<Nonce, ChainMessage>>);

impl TransactionBuffer {
    /// Insert a transaction we could not submit straight away into the buffer.
    pub fn insert(&self, sender: Address, nonce: Nonce, msg: ChainMessage) {
        self.0.with(|c| {
            let buffer = c.entry(sender).or_insert_with(BTreeMap::new);
            // Overwrite any previous entry to protect against DoS attack; it wouldn't make sense to submit them anyway.
            buffer.insert(nonce, msg);
        })
    }

    /// Remove all (sender, nonce) pairs which were included in a block.
    fn remove_many<'a, I>(&self, txs: I)
    where
        I: Iterator<Item = (&'a Address, Nonce)>,
    {
        self.0.with(|c| {
            for (sender, nonce) in txs {
                if let Some(buffer) = c.get_mut(sender) {
                    buffer.remove(&nonce);
                }
            }
        })
    }

    /// Gather any messages that have been enabled by transactions added to a block.
    ///
    /// These are removed from the cache, submission is only attempted once.
    fn remove_unblocked<'a, I>(&self, txs: I) -> Vec<(Address, Nonce, ChainMessage)>
    where
        I: Iterator<Item = (&'a Address, Nonce)>,
    {
        self.0.with(|c| {
            let mut msgs = Vec::new();
            for (sender, mut nonce) in txs {
                if let Some(buffer) = c.get_mut(sender) {
                    nonce += 1;
                    while let Some(msg) = buffer.remove(&nonce) {
                        msgs.push((*sender, nonce, msg));
                        nonce += 1;
                    }
                }
            }
            msgs
        })
    }
}

/// Subscribe to `NewBlock`  notifications and clear transactions from the caches.`
pub fn start_tx_cache_clearing(
    client: FendermintClient<HybridClient>,
    tx_cache: TransactionCache,
    tx_buffer: TransactionBuffer,
) {
    tokio::task::spawn(async move {
        let chain_id = get_chain_id(&client).await;
        tx_cache_clearing_loop(client.into_underlying(), chain_id, tx_cache, tx_buffer).await;
    });
}

/// Subscribe to notifications about new blocks and
/// 1) remove all included transactions from the caches
/// 2) broadcast buffered out-of-order transactions when they are unblocked
///
/// Re-subscribe in the event of a subscription failure.
async fn tx_cache_clearing_loop<C>(
    client: C,
    chain_id: ChainID,
    tx_cache: TransactionCache,
    tx_buffer: TransactionBuffer,
) where
    C: Client + SubscriptionClient + Send + Sync,
{
    loop {
        let query = Query::from(EventType::NewBlock);

        match client.subscribe(query).await {
            Err(e) => {
                tracing::warn!(error=?e, "failed to subscribe to NewBlocks; retrying later...");
                tokio::time::sleep(Duration::from_secs(RETRY_SLEEP_SECS)).await;
            }
            Ok(mut subscription) => {
                while let Some(result) = subscription.next().await {
                    match result {
                        Err(e) => {
                            tracing::warn!(error=?e, "NewBlocks subscription failed; resubscribing...");
                            break;
                        }
                        Ok(event) => {
                            if let EventData::NewBlock {
                                block: Some(block), ..
                            } = event.data
                            {
                                let txs = collect_txs(&block, &chain_id);

                                if txs.is_empty() {
                                    continue;
                                }

                                let tx_hashes = txs.iter().map(|(h, _, _)| h);
                                let tx_nonces = || txs.iter().map(|(_, s, n)| (s, *n));

                                tx_cache.remove_many(tx_hashes);
                                // First remove all transactions which have been in the block (could be multiple from the same sender).
                                tx_buffer.remove_many(tx_nonces());
                                // Then collect whatever is unblocked on top of those, ie. anything that hasn't been included, but now can.
                                let unblocked_msgs = tx_buffer.remove_unblocked(tx_nonces());
                                // Send them all with best-effort.
                                send_msgs(&client, unblocked_msgs).await;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Collect the identifiers of the transactions in the block.
fn collect_txs(block: &Block, chain_id: &ChainID) -> Vec<(et::TxHash, Address, Nonce)> {
    let mut txs = Vec::new();
    for tx in &block.data {
        if let Ok(ChainMessage::Signed(msg)) = fvm_ipld_encoding::from_slice(tx) {
            if let Ok(Some(DomainHash::Eth(h))) = msg.domain_hash(chain_id) {
                txs.push((et::TxHash::from(h), msg.message.from, msg.message.sequence))
            }
        }
    }
    txs
}

/// Fetch the chain ID from the API; do it in a loop until it succeeds.
async fn get_chain_id(client: &FendermintClient<HybridClient>) -> ChainID {
    loop {
        match client.state_params(FvmQueryHeight::default()).await {
            Ok(sp) => {
                return ChainID::from(sp.value.chain_id);
            }
            Err(e) => {
                tracing::warn!(error=?e, "failed to get chain ID; retrying later...");
                tokio::time::sleep(Duration::from_secs(RETRY_SLEEP_SECS)).await;
            }
        }
    }
}

/// Best effort attempt to broadcast previously out-of-order transactions which have been unblocked.
async fn send_msgs<C>(client: &C, msgs: Vec<(Address, Nonce, ChainMessage)>)
where
    C: Client + Send + Sync,
{
    for (sender, nonce, msg) in msgs {
        let Ok(bz) = SignedMessageFactory::serialize(&msg) else {
            continue;
        };

        // Use the broadcast version which waits for basic checks to complete.
        match client.broadcast_tx_sync(bz).await {
            Ok(_) => {
                tracing::info!(
                    sender = sender.to_string(),
                    nonce,
                    "submitted out-of-order transaction"
                );
            }
            Err(e) => {
                tracing::error!(error=?e, sender = sender.to_string(), nonce, "failed to submit out-of-order transaction");
            }
        }
    }
}
