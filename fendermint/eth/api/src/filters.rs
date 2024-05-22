// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context};
use ethers_core::types as et;
use fendermint_rpc::{client::FendermintClient, query::QueryClient};
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_message::{chain::ChainMessage, query::FvmQueryHeight, signed::DomainHash};
use futures::{Future, StreamExt};
use fvm_shared::{address::Address, chainid::ChainID, error::ExitCode};
use lru_time_cache::LruCache;
use serde::Serialize;
use tendermint_rpc::{
    event::{Event, EventData},
    query::{EventType, Query},
    Client, Subscription,
};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    RwLock,
};

use crate::{
    conv::from_tm::{self, find_hash_event, map_rpc_block_txs, msg_hash, tx_hash},
    error::JsonRpcError,
    handlers::ws::{MethodNotification, Notification},
    state::{enrich_block, WebSocketSender},
    JsonRpcResult,
};

/// Check whether to keep a log according to the topic filter.
///
/// A note on specifying topic filters: Topics are order-dependent.
/// A transaction with a log with topics [A, B] will be matched by the following topic filters:
/// * [] "anything"
/// * [A] "A in first position (and anything after)"
/// * [null, B] "anything in first position AND B in second position (and anything after)"
/// * [A, B] "A in first position AND B in second position (and anything after)"
/// * [[A, B], [A, B]] "(A OR B) in first position AND (A OR B) in second position (and anything after)"
pub fn matches_topics(filter: &et::Filter, log: &et::Log) -> bool {
    for i in 0..4 {
        if let Some(topics) = &filter.topics[i] {
            let topic = log.topics.get(i);
            let matches = match topics {
                et::ValueOrArray::Value(Some(t)) => topic == Some(t),
                et::ValueOrArray::Array(ts) => ts.iter().flatten().any(|t| topic == Some(t)),
                _ => true,
            };
            if !matches {
                return false;
            }
        }
    }
    true
}

pub type FilterId = et::U256;
pub type FilterMap = Arc<RwLock<HashMap<FilterId, Sender<FilterCommand>>>>;

pub type BlockHash = et::H256;

pub enum FilterCommand {
    /// Update the records with an event, coming from one of the Tendermint subscriptions.
    Update(Event),
    /// One of the subscriptions has ended, potentially with an error.
    Finish(Option<tendermint_rpc::Error>),
    /// Take the accumulated records, coming from the API consumer.
    Take(tokio::sync::oneshot::Sender<anyhow::Result<Option<FilterRecords<BlockHash>>>>),
    /// The API consumer is no longer interested in taking the records.
    Uninstall,
}

pub enum FilterKind {
    NewBlocks,
    PendingTransactions,
    Logs(Box<et::Filter>),
}

impl FilterKind {
    /// Convert an Ethereum filter to potentially multiple Tendermint queries.
    ///
    /// One limitation with Tendermint is that it only handles AND condition
    /// in filtering, so if the filter contains arrays, we have to make a
    /// cartesian product of all conditions in it and subscribe individually.
    ///
    /// https://docs.tendermint.com/v0.34/rpc/#/Websocket/subscribe
    pub fn to_queries(&self) -> Vec<Query> {
        match self {
            FilterKind::NewBlocks => vec![Query::from(EventType::NewBlock)],
            // Testing indicates that `EventType::Tx` might only be raised
            // if there are events emitted by the transaction itself.
            FilterKind::PendingTransactions => vec![Query::from(EventType::NewBlock)],
            FilterKind::Logs(filter) => {
                // `Query::from(EventType::Tx)` doesn't seem to combine well with non-standard keys.
                // But `Query::default()` doesn't return anything if we subscribe to `Filter::default()`.
                let mut query = if filter.has_topics() || filter.address.is_some() {
                    Query::default()
                } else {
                    Query::from(EventType::Tx)
                };

                if let Some(_block_hash) = filter.get_block_hash() {
                    // Currently we only use these filters for subscribing to future events,
                    // we don't go back to retireve past ones (although I think Lotus does that).
                    // As such, it is impossible to subscribe to future block hashes, they are unknown.
                    // We could add a `block.hash` to the index, but there are other ways to find transactions
                    // in a block, so it would be storing data for little reason.
                }
                if let Some(from_block) = filter.get_from_block() {
                    query = query.and_gte("tx.height", from_block.as_u64());
                }
                if let Some(to_block) = filter.get_to_block() {
                    query = query.and_lte("tx.height", to_block.as_u64());
                }

                let mut queries = vec![query];

                let addrs = match &filter.address {
                    None => vec![],
                    Some(et::ValueOrArray::Value(addr)) => vec![*addr],
                    Some(et::ValueOrArray::Array(addrs)) => addrs.clone(),
                };

                // We need to turn the Ethereum addresses f410 addresses, which is something we asked CometBFT to index
                // so that we can use it for filtering.
                let addrs = addrs
                    .into_iter()
                    .map(|addr| Address::from(EthAddress(addr.0)))
                    .collect::<Vec<_>>();

                if !addrs.is_empty() {
                    queries = addrs
                        .iter()
                        .flat_map(|addr| {
                            queries.iter().flat_map(|q| {
                                let mut emitters = if let Ok(id) = addr.id() {
                                    // If it was a masked ID.
                                    vec![q.clone().and_eq("event.emitter.id", id.to_string())]
                                } else {
                                    vec![q.clone().and_eq("event.emitter.deleg", addr.to_string())]
                                };
                                emitters.push(q.clone().and_eq("message.from", addr.to_string()));
                                emitters.push(q.clone().and_eq("message.to", addr.to_string()));
                                emitters
                            })
                        })
                        .collect();
                };

                for i in 0..4 {
                    if let Some(Some(topics)) = filter.topics.get(i) {
                        let topics = match topics {
                            et::ValueOrArray::Value(Some(t)) => vec![t],
                            et::ValueOrArray::Array(ts) => ts.iter().flatten().collect(),
                            _ => vec![],
                        };
                        if !topics.is_empty() {
                            let key = format!("event.t{}", i + 1);
                            queries = topics
                                .into_iter()
                                .flat_map(|t| {
                                    queries
                                        .iter()
                                        .map(|q| q.clone().and_eq(&key, hex::encode(t.0)))
                                })
                                .collect();
                        }
                    }
                }

                queries
            }
        }
    }
}

/// Accumulator for filter data.
///
/// The type expected can be seen in [ethers::providers::Provider::watch_blocks].
pub enum FilterRecords<B> {
    NewBlocks(Vec<B>),
    PendingTransactions(Vec<et::TxHash>),
    Logs(Vec<et::Log>),
}

impl<B> FilterRecords<B>
where
    B: Serialize,
{
    pub fn new(value: &FilterKind) -> Self {
        match value {
            FilterKind::NewBlocks => Self::NewBlocks(vec![]),
            FilterKind::PendingTransactions => Self::PendingTransactions(vec![]),
            FilterKind::Logs(_) => Self::Logs(vec![]),
        }
    }

    fn take(&mut self) -> Self {
        let mut records = match self {
            Self::NewBlocks(_) => Self::NewBlocks(vec![]),
            Self::PendingTransactions(_) => Self::PendingTransactions(vec![]),
            Self::Logs(_) => Self::Logs(vec![]),
        };
        std::mem::swap(self, &mut records);
        records
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::NewBlocks(xs) => xs.is_empty(),
            Self::PendingTransactions(xs) => xs.is_empty(),
            Self::Logs(xs) => xs.is_empty(),
        }
    }

    pub fn to_json_vec(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        match self {
            Self::Logs(xs) => to_json_vec(xs),
            Self::NewBlocks(xs) => to_json_vec(xs),
            Self::PendingTransactions(xs) => to_json_vec(xs),
        }
    }

    /// Accumulate the events.
    async fn update<F>(
        &mut self,
        event: Event,
        to_block: F,
        chain_id: &ChainID,
        filter: &Option<et::Filter>,
    ) -> anyhow::Result<()>
    where
        F: FnOnce(tendermint::Block) -> Pin<Box<dyn Future<Output = anyhow::Result<B>> + Send>>,
    {
        match (self, event.data) {
            (
                Self::NewBlocks(ref mut blocks),
                EventData::NewBlock {
                    block: Some(block), ..
                },
            ) => {
                let b: B = to_block(block).await?;
                blocks.push(b);
            }
            (
                Self::PendingTransactions(ref mut hashes),
                EventData::NewBlock {
                    block: Some(block), ..
                },
            ) => {
                for tx in &block.data {
                    if let Ok(ChainMessage::Signed(msg)) = fvm_ipld_encoding::from_slice(tx) {
                        if let Ok(Some(DomainHash::Eth(h))) = msg.domain_hash(chain_id) {
                            hashes.push(et::TxHash::from(h))
                        }
                    }
                }
            }
            (Self::Logs(ref mut logs), EventData::Tx { tx_result }) => {
                // An example of an `Event`:
                // Event {
                //     query: "tm.event = 'Tx'",
                //     data: Tx {
                //         tx_result: TxInfo {
                //             height: 1088,
                //             index: None,
                //             tx: [161, 102, ..., 0],
                //             result: TxResult {
                //                 log: None,
                //                 gas_wanted: Some("5156433"),
                //                 gas_used: Some("5151233"),
                //                 events: [
                //                     Event {
                //                         kind: "event",
                //                         attributes: [
                //                             EventAttribute { key: "emitter.id", value: "108", index: true },
                //                             EventAttribute { key: "t1", value: "dd...b3ef", index: true },
                //                             EventAttribute { key: "t2", value: "00...362f", index: true },
                //                             EventAttribute { key: "t3", value: "00...44eb", index: true },
                //                             EventAttribute { key: "d",  value: "00...0064", index: true }
                //                         ]
                //                     }
                //                 ]
                //             }
                //         }
                //     },
                //     events: Some(
                //     {
                //         "event.d": ["00...0064"],
                //         "event.emitter.id": ["108"],
                //         "event.t1": ["dd...b3ef"],
                //         "event.t2": ["00...362f"],
                //         "event.t3": ["00...44eb"],
                //         "tm.event": ["Tx"],
                //         "tx.hash": ["FA7339B4D9F6AF80AEDB03FC4BFBC1FDD9A62F97632EF8B79C98AAD7044C5BDB"],
                //         "tx.height": ["1088"]
                //     })
                // }

                // There is no easy way here to tell the block hash. Maybe it has been given in a preceding event,
                // but other than that our only option is to query the Tendermint API. If we do that we should have caching,
                // otherwise all the transactions in a block hammering the node will act like a DoS attack.
                // Or we can add it to the indexed fields.
                let block_hash =
                    find_hash_event("block", &tx_result.result.events).unwrap_or_default();

                let block_number = et::U64::from(tx_result.height);

                let transaction_hash = msg_hash(&tx_result.result.events, &tx_result.tx);

                // TODO: The transaction index comes as None.
                let transaction_index = et::U64::from(tx_result.index.unwrap_or_default());

                // TODO: We have no way to tell where the logs start within the block.
                let log_index_start = Default::default();

                let mut tx_logs = from_tm::to_logs(
                    &tx_result.result.events,
                    block_hash,
                    block_number,
                    transaction_hash,
                    transaction_index,
                    log_index_start,
                )?;

                if let Some(filter) = filter {
                    tx_logs.retain(|log| matches_topics(filter, log));
                }

                logs.extend(tx_logs)
            }
            _ => {}
        }
        Ok(())
    }
}

fn to_json_vec<R: Serialize>(records: &[R]) -> anyhow::Result<Vec<serde_json::Value>> {
    let values: Vec<serde_json::Value> = records
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .context("failed to convert records to JSON")?;

    Ok(values)
}

pub struct FilterDriver {
    id: FilterId,
    kind: FilterKind,
    state: FilterState,
    rx: Receiver<FilterCommand>,
}

enum FilterState {
    Poll(PollState),
    Subscription(SubscriptionState),
}

/// Accumulate changes between polls.
///
/// Polling returns batches.
struct PollState {
    timeout: Duration,
    last_poll: Instant,
    finished: Option<Option<anyhow::Error>>,
    records: FilterRecords<BlockHash>,
}

/// Send changes to a WebSocket as soon as they happen, one by one, not in batches.
struct SubscriptionState {
    ws_sender: WebSocketSender,
}

impl FilterDriver {
    pub fn new(
        id: FilterId,
        timeout: Duration,
        kind: FilterKind,
        ws_sender: Option<WebSocketSender>,
    ) -> (Self, Sender<FilterCommand>) {
        let (tx, rx) = tokio::sync::mpsc::channel(10);

        let state = match ws_sender {
            Some(ws_sender) => FilterState::Subscription(SubscriptionState { ws_sender }),
            None => FilterState::Poll(PollState {
                timeout,
                last_poll: Instant::now(),
                finished: None,
                records: FilterRecords::new(&kind),
            }),
        };

        let r = Self {
            id,
            kind,
            state,
            rx,
        };

        (r, tx)
    }

    pub fn id(&self) -> FilterId {
        self.id
    }

    /// Consume commands until some end condition is met.
    ///
    /// In the end the filter removes itself from the registry.
    pub async fn run<C>(mut self, filters: FilterMap, client: FendermintClient<C>)
    where
        C: Client + Send + Sync + Clone + 'static,
    {
        let id = self.id;

        tracing::info!(?id, "handling filter events");

        // Get the Chain ID once. In practice it will not change and will last the entire session.
        let chain_id = client
            .state_params(FvmQueryHeight::default())
            .await
            .map(|state_params| ChainID::from(state_params.value.chain_id));

        // Logs need to be filtered by topics.
        let filter = if let FilterKind::Logs(ref filter) = self.kind {
            Some(filter.as_ref().to_owned())
        } else {
            None
        };

        // Because there are multiple potentially overlapping subscriptions, we might see the same transaction twice,
        // e.g. because we were interested in ones that emit events "A or B" we had to subscribe to "A" and also to "B",
        // so if a transaction emits both "A" and "B" we'll get it twice. Most likely they will be at the same time,
        // so a short time based cache should help get rid of the duplicates.
        let mut tx_cache: LruCache<tendermint::Hash, bool> =
            LruCache::with_expiry_duration(Duration::from_secs(60));

        while let Some(cmd) = self.rx.recv().await {
            // Skip duplicate transactions. We won't see duplidate blocks because there is only 1 query for that.
            if let FilterCommand::Update(ref event) = cmd {
                if let EventData::Tx { ref tx_result } = event.data {
                    let tx_hash = tx_hash(&tx_result.tx);
                    if tx_cache.insert(tx_hash, true).is_some() {
                        continue;
                    }
                }
            }

            match self.state {
                FilterState::Poll(ref mut state) => {
                    match cmd {
                        FilterCommand::Update(event) => {
                            if state.is_timed_out() {
                                tracing::debug!(?id, "filter timed out");
                                return self.remove(filters).await;
                            }
                            if state.is_finished() {
                                // Not returning to allow the consumer to get final results.
                                continue;
                            }

                            let res = match &chain_id {
                                Ok(chain_id) => {
                                    state
                                        .records
                                        .update(
                                            event,
                                            |block| {
                                                Box::pin(async move {
                                                    Ok(et::H256::from_slice(
                                                        block.header().hash().as_bytes(),
                                                    ))
                                                })
                                            },
                                            chain_id,
                                            &filter,
                                        )
                                        .await
                                }
                                Err(e) => Err(anyhow!("failed to get chain ID: {e}")),
                            };

                            if let Err(err) = res {
                                tracing::error!(?id, "failed to update filter: {err}");
                                state.finish(Some(anyhow!("failed to update filter: {err}")));
                            }
                        }
                        FilterCommand::Finish(err) => {
                            tracing::debug!(?id, "filter producer finished: {err:?}");
                            state.finish(err.map(|e| anyhow!("subscription failed: {e}")))
                        }
                        FilterCommand::Take(tx) => {
                            let result = state.try_take();
                            let remove = match result {
                                Ok(None) | Err(_) => true,
                                Ok(Some(_)) => false,
                            };
                            let _ = tx.send(result);
                            if remove {
                                tracing::debug!(?id, "filter finished");
                                return self.remove(filters).await;
                            }
                        }
                        FilterCommand::Uninstall => {
                            tracing::debug!(?id, "filter uninstalled");
                            return self.remove(filters).await;
                        }
                    }
                }
                FilterState::Subscription(ref state) => match cmd {
                    FilterCommand::Update(event) => {
                        let mut records = FilterRecords::<et::Block<et::TxHash>>::new(&self.kind);

                        let res = match &chain_id {
                            Ok(chain_id) => {
                                records
                                    .update(
                                        event,
                                        |block| {
                                            let client = client.clone();
                                            Box::pin(async move {
                                                let block =
                                                    enrich_block_with_retry(&client, &block)
                                                        .await
                                                        .context(
                                                            "failed to enrich block in event",
                                                        )?;
                                                let block: anyhow::Result<et::Block<et::TxHash>> =
                                                    map_rpc_block_txs(block, |tx| Ok(tx.hash()));
                                                block
                                            })
                                        },
                                        chain_id,
                                        &filter,
                                    )
                                    .await
                            }
                            Err(e) => Err(anyhow!("failed to get chain ID: {e}")),
                        };

                        match res {
                            Err(e) => {
                                send_error(
                                    &state.ws_sender,
                                    ExitCode::USR_UNSPECIFIED,
                                    format!("failed to process events: {e}"),
                                    id,
                                );
                            }
                            Ok(()) => match records.to_json_vec() {
                                Err(e) => tracing::error!("failed to convert events to JSON: {e}"),
                                Ok(records) => {
                                    for rec in records {
                                        let msg: MethodNotification = notification(id, rec);
                                        if state.ws_sender.send(msg).is_err() {
                                            tracing::debug!(?id, "web socket no longer listening");
                                            return self.remove(filters).await;
                                        }
                                    }
                                }
                            },
                        }
                    }
                    FilterCommand::Finish(err) => {
                        tracing::debug!(?id, "subscription producer finished: {err:?}");
                        // We have already sent all updates to the socket.

                        // Make best effort to notify the socket.
                        if let Some(err) = err {
                            send_error(
                                &state.ws_sender,
                                ExitCode::USR_UNSPECIFIED,
                                format!("subscription finished with error: {err}"),
                                id,
                            );
                        }

                        // We know at least one subscription has failed, so might as well quit.
                        return self.remove(filters).await;
                    }
                    FilterCommand::Take(tx) => {
                        // This should not be used, but because we treat subscriptions and filters
                        // under the same umbrella, it is possible to send a request to get changes.
                        // Respond with empty, because all of the changes were already sent to the socket.
                        let _ = tx.send(Ok(Some(FilterRecords::new(&self.kind))));
                    }
                    FilterCommand::Uninstall => {
                        tracing::debug!(?id, "subscription uninstalled");
                        return self.remove(filters).await;
                    }
                },
            }
        }
    }

    async fn remove(self, filters: FilterMap) {
        filters.write().await.remove(&self.id);
    }
}

fn send_error(ws_sender: &WebSocketSender, exit_code: ExitCode, msg: String, id: FilterId) {
    tracing::error!(?id, "sending error to WS: {msg}");

    let err = JsonRpcError {
        code: exit_code.value().into(),
        message: msg,
        data: None,
    };
    let err = jsonrpc_v2::Error::from(err);

    match serde_json::to_value(err) {
        Err(e) => tracing::error!("failed to convert JSON-RPC error to JSON: {e}"),
        Ok(json) => {
            // Ignoring the case where the socket is no longer there.
            // Assuming that there will be another event to trigger removal.
            let msg = notification(id, json);
            let _ = ws_sender.send(msg);
        }
    }
}

fn notification(subscription: FilterId, result: serde_json::Value) -> MethodNotification {
    MethodNotification {
        // We know this is the only one at the moment.
        // The go-ethereum client checks that the suffix is "_subscription":
        // https://github.com/ethereum/go-ethereum/blob/92b8f28df3255c6cef9605063850d77b46146763/rpc/handler.go#L236C42-L236C42
        method: "eth_subscription".into(),
        notification: Notification {
            subscription,
            result,
        },
    }
}

/// It looks like it might not be true that when we receive a `NewBlock` event from Tendermint,
/// (which includes begin and end events, so we can assume it's been executed), then it's safe
/// to query the API for the block results, which is what `enrich_block` does to fill stuff
/// like gas used, etc.
async fn enrich_block_with_retry<C: Client + Send + Sync>(
    client: &FendermintClient<C>,
    block: &tendermint::block::Block,
) -> JsonRpcResult<et::Block<et::Transaction>> {
    // TODO: Assuming at ~1 block time; move this to config.
    const SLEEP_SECS: u64 = 1;
    const MAX_ATTEMPT: u32 = 5;
    let mut attempt = 0;
    loop {
        match enrich_block(client, block).await {
            Err(e) if attempt < MAX_ATTEMPT => {
                tracing::debug!(
                    error = e.to_string(),
                    height = block.header().height.value(),
                    "failed to enrich block; retrying..."
                );
                tokio::time::sleep(Duration::from_secs(SLEEP_SECS)).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

impl PollState {
    /// Take all the accumulated changes.
    ///
    /// If there are no changes but there was an error, return that.
    /// If the producers have stopped, return `None`.
    fn try_take(&mut self) -> anyhow::Result<Option<FilterRecords<BlockHash>>> {
        self.last_poll = Instant::now();

        let records = self.records.take();

        if records.is_empty() {
            if let Some(ref mut finished) = self.finished {
                // Return error on first poll, because it can't be cloned.
                return match finished.take() {
                    Some(e) => Err(e),
                    None => Ok(None),
                };
            }
        }

        Ok(Some(records))
    }

    /// Signal that the producers are finished, or that the reader is no longer intersted.
    ///
    /// Propagate the error to the reader next time it comes to check on the filter.
    fn finish(&mut self, error: Option<anyhow::Error>) {
        // Keep any already existing error.
        let error = self.finished.take().flatten().or(error);

        self.finished = Some(error);
    }

    /// Indicate whether the reader has been too slow at polling the filter
    /// and that it should be removed.
    fn is_timed_out(&self) -> bool {
        Instant::now().duration_since(self.last_poll) > self.timeout
    }

    /// Indicate that that the filter takes no more data.
    fn is_finished(&self) -> bool {
        self.finished.is_some()
    }
}

/// Spawn a Tendermint subscription handler in a new task.
///
/// The subscription sends [Event] records to the driver over a channel.
pub async fn run_subscription(id: FilterId, mut sub: Subscription, tx: Sender<FilterCommand>) {
    let query = sub.query().to_string();
    tracing::debug!(?id, query, "polling filter subscription");
    while let Some(result) = sub.next().await {
        match result {
            Ok(event) => {
                if tx.send(FilterCommand::Update(event)).await.is_err() {
                    // Filter has been uninstalled.
                    tracing::debug!(
                        ?id,
                        query,
                        "filter no longer listening, quiting subscription"
                    );
                    return;
                }
            }
            Err(err) => {
                tracing::error!(
                    ?id,
                    query,
                    error = ?err,
                    "filter subscription error"
                );
                let _ = tx.send(FilterCommand::Finish(Some(err))).await;
                return;
            }
        }
    }
    tracing::debug!(?id, query, "filter subscription finished");
    let _ = tx.send(FilterCommand::Finish(None)).await;

    // Dropping the `Subscription` should cause the client to unsubscribe,
    // if this was the last one interested in that query; we don't have to
    // call the unsubscribe method explicitly.
    // See https://docs.rs/tendermint-rpc/0.31.1/tendermint_rpc/client/struct.WebSocketClient.html
}

#[cfg(test)]
mod tests {
    use ethers_core::types as et;

    use super::FilterKind;

    #[test]
    fn default_filter_to_query() {
        let filter = et::Filter::default();

        let queries = FilterKind::Logs(Box::new(filter)).to_queries();

        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].to_string(), "tm.event = 'Tx'");
    }

    #[test]
    fn filter_to_query() {
        fn hash(s: &str) -> et::H256 {
            et::H256::from(ethers_core::utils::keccak256(s))
        }

        fn hash_hex(s: &str) -> String {
            hex::encode(hash(s))
        }

        let filter = et::Filter::new()
            .select(1234..)
            .address(
                "0xb794f5ea0ba39494ce839613fffba74279579268"
                    .parse::<et::Address>()
                    .unwrap(),
            )
            .events(vec!["Foo", "Bar"])
            .topic1(hash("Alice"))
            .topic2(
                vec!["Bob", "Charlie"]
                    .into_iter()
                    .map(hash)
                    .collect::<Vec<_>>(),
            );

        eprintln!("filter = {filter:?}");

        assert_eq!(
            filter.topics[0],
            Some(et::ValueOrArray::Array(vec![
                Some(hash("Foo")),
                Some(hash("Bar"))
            ]))
        );

        let queries = FilterKind::Logs(Box::new(filter)).to_queries();

        assert_eq!(queries.len(), 12);

        let mut i = 0;
        for t3 in ["Bob", "Charlie"] {
            for t1 in ["Foo", "Bar"] {
                for addr in ["event.emitter.deleg", "message.from", "message.to"] {
                    let q = queries[i].to_string();
                    let e = format!("tx.height >= 1234 AND {addr} = 'f410fw6kpl2qluokjjtudsyj7765hij4vpetitn2e2wq' AND event.t1 = '{}' AND event.t2 = '{}' AND event.t3 = '{}'", hash_hex(t1), hash_hex("Alice"), hash_hex(t3));
                    assert_eq!(q, e, "combination {i}");
                    i += 1;
                }
            }
        }
    }
}
