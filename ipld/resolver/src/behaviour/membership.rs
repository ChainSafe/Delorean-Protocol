// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use std::collections::{HashMap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::anyhow;
use ipc_api::subnet_id::SubnetID;
use libp2p::core::Endpoint;
use libp2p::gossipsub::{
    self, IdentTopic, MessageAuthenticity, MessageId, PublishError, Sha256Topic, SubscriptionError,
    Topic, TopicHash,
};
use libp2p::identity::Keypair;
use libp2p::swarm::derive_prelude::FromSwarm;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use log::{debug, error, info, warn};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::time::{Instant, Interval};

use crate::hash::blake2b_256;
use crate::provider_cache::{ProviderDelta, SubnetProviderCache};
use crate::provider_record::{ProviderRecord, SignedProviderRecord};
use crate::vote_record::{SignedVoteRecord, VoteRecord};
use crate::{stats, Timestamp};

use super::NetworkConfig;

/// `Gossipsub` topic identifier for subnet membership.
const PUBSUB_MEMBERSHIP: &str = "/ipc/membership";
/// `Gossipsub` topic identifier for voting about content.
const PUBSUB_VOTES: &str = "/ipc/ipld/votes";
/// `Gossipsub` topic identifier for pre-emptively published blocks of data.
const PUBSUB_PREEMPTIVE: &str = "/ipc/ipld/pre-emptive";

/// Events emitted by the [`membership::Behaviour`] behaviour.
#[derive(Debug)]
pub enum Event<V> {
    /// Indicate a change in the subnets a peer is known to support.
    Updated(PeerId, ProviderDelta),

    /// Indicate that we no longer treat a peer as routable and removed all their supported subnet associations.
    Removed(PeerId),

    /// We could not add a provider record to the cache because the chache hasn't
    /// been told yet that the provider peer is routable. This event can be used
    /// to trigger a lookup by the discovery module to learn the address.
    Skipped(PeerId),

    /// We received a [`VoteRecord`] in one of the subnets we are providing data for.
    ReceivedVote(Box<VoteRecord<V>>),

    /// We received preemptive data published in a subnet we were interested in.
    ReceivedPreemptive(SubnetID, Vec<u8>),
}

/// Configuration for [`membership::Behaviour`].
#[derive(Clone, Debug)]
pub struct Config {
    /// User defined list of subnets which will never be pruned from the cache.
    pub static_subnets: Vec<SubnetID>,
    /// Maximum number of subnets to track in the cache.
    pub max_subnets: usize,
    /// Publish interval for supported subnets.
    pub publish_interval: Duration,
    /// Minimum time between publishing own provider record in reaction to new joiners.
    pub min_time_between_publish: Duration,
    /// Maximum age of provider records before the peer is removed without an update.
    pub max_provider_age: Duration,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("invalid network: {0}")]
    InvalidNetwork(String),
    #[error("invalid gossipsub config: {0}")]
    InvalidGossipsubConfig(String),
    #[error("error subscribing to topic")]
    Subscription(#[from] SubscriptionError),
}

/// A [`NetworkBehaviour`] internally using [`Gossipsub`] to learn which
/// peer is able to resolve CIDs in different subnets.
pub struct Behaviour<V> {
    /// [`gossipsub::Behaviour`] to spread the information about subnet membership.
    inner: gossipsub::Behaviour,
    /// Events to return when polled.
    outbox: VecDeque<Event<V>>,
    /// [`Keypair`] used to sign [`SignedProviderRecord`] instances.
    local_key: Keypair,
    /// Name of the P2P network, used to separate `Gossipsub` topics.
    network_name: String,
    /// Name of the [`Gossipsub`] topic where subnet memberships are published.
    membership_topic: IdentTopic,
    /// List of subnet IDs this agent is providing data for.
    subnet_ids: Vec<SubnetID>,
    /// Voting topics we are currently subscribed to.
    voting_topics: HashSet<TopicHash>,
    /// Remember which subnet a topic was about.
    preemptive_topics: HashMap<TopicHash, SubnetID>,
    /// Caching the latest state of subnet providers.
    provider_cache: SubnetProviderCache,
    /// Interval between publishing the currently supported subnets.
    ///
    /// This acts like a heartbeat; if a peer doesn't publish its snapshot for a long time,
    /// other agents can prune it from their cache and not try to contact for resolution.
    publish_interval: Interval,
    /// Minimum time between publishing own provider record in reaction to new joiners.
    min_time_between_publish: Duration,
    /// Last time we gossiped our own provider record.
    last_publish_timestamp: Timestamp,
    /// Next time we will gossip our own provider record.
    next_publish_timestamp: Timestamp,
    /// Maximum time a provider can be without an update before it's pruned from the cache.
    max_provider_age: Duration,
    _phantom_vote: PhantomData<V>,
}

impl<V> Behaviour<V>
where
    V: Serialize + DeserializeOwned,
{
    pub fn new(nc: NetworkConfig, mc: Config) -> Result<Self, ConfigError> {
        if nc.network_name.is_empty() {
            return Err(ConfigError::InvalidNetwork(nc.network_name));
        }
        let membership_topic = Topic::new(format!("{}/{}", PUBSUB_MEMBERSHIP, nc.network_name));

        let mut gossipsub_config = gossipsub::ConfigBuilder::default();
        // Set the maximum message size to 2MB.
        gossipsub_config.max_transmit_size(2 << 20);
        gossipsub_config.message_id_fn(|msg: &gossipsub::Message| {
            let s = blake2b_256(&msg.data);
            MessageId::from(s)
        });

        let gossipsub_config = gossipsub_config
            .build()
            .map_err(|s| ConfigError::InvalidGossipsubConfig(s.to_string()))?;

        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(nc.local_key.clone()),
            gossipsub_config,
        )
        .map_err(|s| ConfigError::InvalidGossipsubConfig(s.into()))?;

        gossipsub
            .with_peer_score(
                scoring::build_peer_score_params(membership_topic.clone()),
                scoring::build_peer_score_thresholds(),
            )
            .map_err(ConfigError::InvalidGossipsubConfig)?;

        // Subscribe to the topic.
        gossipsub.subscribe(&membership_topic)?;

        // Don't publish immediately, it's empty. Let the creator call `set_subnet_ids` to trigger initially.
        let mut interval = tokio::time::interval(mc.publish_interval);
        interval.reset();

        // Not passing static subnets here; using pinning below instead so it subscribes as well
        let provider_cache = SubnetProviderCache::new(mc.max_subnets, vec![]);

        let mut membership = Self {
            inner: gossipsub,
            outbox: Default::default(),
            local_key: nc.local_key,
            network_name: nc.network_name,
            membership_topic,
            subnet_ids: Default::default(),
            voting_topics: Default::default(),
            preemptive_topics: Default::default(),
            provider_cache,
            publish_interval: interval,
            min_time_between_publish: mc.min_time_between_publish,
            last_publish_timestamp: Timestamp::default(),
            next_publish_timestamp: Timestamp::now() + mc.publish_interval,
            max_provider_age: mc.max_provider_age,
            _phantom_vote: PhantomData,
        };

        for subnet_id in mc.static_subnets {
            membership.pin_subnet(subnet_id)?;
        }

        Ok(membership)
    }

    fn subscribe(&mut self, topic: &Sha256Topic) -> Result<bool, SubscriptionError> {
        info!("subscribing to ${topic}");
        self.inner.subscribe(topic)
    }

    fn unsubscribe(&mut self, topic: &Sha256Topic) -> Result<bool, PublishError> {
        info!("unsubscribing from ${topic}");
        self.inner.unsubscribe(topic)
    }

    /// Construct the topic used to gossip about pre-emptively published data.
    ///
    /// Replaces "/" with "_" to avoid clashes from prefix/suffix overlap.
    fn preemptive_topic(&self, subnet_id: &SubnetID) -> Sha256Topic {
        Topic::new(format!(
            "{}/{}/{}",
            PUBSUB_PREEMPTIVE,
            self.network_name.replace('/', "_"),
            subnet_id.to_string().replace('/', "_")
        ))
    }

    /// Subscribe to a preemptive topic.
    fn preemptive_subscribe(&mut self, subnet_id: SubnetID) -> Result<(), SubscriptionError> {
        let topic = self.preemptive_topic(&subnet_id);
        self.subscribe(&topic)?;
        self.preemptive_topics.insert(topic.hash(), subnet_id);
        Ok(())
    }

    /// Subscribe to a preemptive topic.
    fn preemptive_unsubscribe(&mut self, subnet_id: &SubnetID) -> anyhow::Result<()> {
        let topic = self.preemptive_topic(subnet_id);
        self.unsubscribe(&topic)?;
        self.preemptive_topics.remove(&topic.hash());
        Ok(())
    }

    /// Construct the topic used to gossip about votes.
    ///
    /// Replaces "/" with "_" to avoid clashes from prefix/suffix overlap.
    fn voting_topic(&self, subnet_id: &SubnetID) -> Sha256Topic {
        Topic::new(format!(
            "{}/{}/{}",
            PUBSUB_VOTES,
            self.network_name.replace('/', "_"),
            subnet_id.to_string().replace('/', "_")
        ))
    }

    /// Subscribe to a voting topic.
    fn voting_subscribe(&mut self, subnet_id: &SubnetID) -> Result<(), SubscriptionError> {
        let topic = self.voting_topic(subnet_id);
        self.subscribe(&topic)?;
        self.voting_topics.insert(topic.hash());
        Ok(())
    }

    /// Unsubscribe from a voting topic.
    fn voting_unsubscribe(&mut self, subnet_id: &SubnetID) -> anyhow::Result<()> {
        let topic = self.voting_topic(subnet_id);
        self.unsubscribe(&topic)?;
        self.voting_topics.remove(&topic.hash());
        Ok(())
    }

    /// Set all the currently supported subnet IDs, then publish the updated list.
    pub fn set_provided_subnets(&mut self, subnet_ids: Vec<SubnetID>) -> anyhow::Result<()> {
        let old_subnet_ids = std::mem::take(&mut self.subnet_ids);
        // Unsubscribe from removed.
        for subnet_id in old_subnet_ids.iter() {
            if !subnet_ids.contains(subnet_id) {
                self.voting_unsubscribe(subnet_id)?;
            }
        }
        // Subscribe to added.
        for subnet_id in subnet_ids.iter() {
            if !old_subnet_ids.contains(subnet_id) {
                self.voting_subscribe(subnet_id)?;
            }
        }
        self.subnet_ids = subnet_ids;
        self.publish_membership()
    }

    /// Add a subnet to the list of supported subnets, then publish the updated list.
    pub fn add_provided_subnet(&mut self, subnet_id: SubnetID) -> anyhow::Result<()> {
        if self.subnet_ids.contains(&subnet_id) {
            return Ok(());
        }
        self.voting_subscribe(&subnet_id)?;
        self.subnet_ids.push(subnet_id);
        self.publish_membership()
    }

    /// Remove a subnet from the list of supported subnets, then publish the updated list.
    pub fn remove_provided_subnet(&mut self, subnet_id: SubnetID) -> anyhow::Result<()> {
        if !self.subnet_ids.contains(&subnet_id) {
            return Ok(());
        }
        self.voting_unsubscribe(&subnet_id)?;
        self.subnet_ids.retain(|id| id != &subnet_id);
        self.publish_membership()
    }

    /// Make sure a subnet is not pruned, so we always track its providers.
    /// Also subscribe to pre-emptively published blocks of data.
    ///
    /// This method could be called in a parent subnet when the ledger indicates
    /// there is a known child subnet, so we make sure this subnet cannot be
    /// crowded out during the initial phase of bootstrapping the network.
    pub fn pin_subnet(&mut self, subnet_id: SubnetID) -> Result<(), SubscriptionError> {
        self.preemptive_subscribe(subnet_id.clone())?;
        self.provider_cache.pin_subnet(subnet_id);
        Ok(())
    }

    /// Make a subnet pruneable and unsubscribe from pre-emptive data.
    pub fn unpin_subnet(&mut self, subnet_id: &SubnetID) -> anyhow::Result<()> {
        self.preemptive_unsubscribe(subnet_id)?;
        self.provider_cache.unpin_subnet(subnet_id);
        Ok(())
    }

    /// Send a message through Gossipsub to let everyone know about the current configuration.
    fn publish_membership(&mut self) -> anyhow::Result<()> {
        let record = ProviderRecord::signed(&self.local_key, self.subnet_ids.clone())?;
        let data = record.into_envelope().into_protobuf_encoding();
        debug!(
            "publishing membership in {:?} to {}",
            self.subnet_ids, self.membership_topic
        );
        match self.inner.publish(self.membership_topic.clone(), data) {
            Err(e) => {
                stats::MEMBERSHIP_PUBLISH_FAILURE.inc();
                Err(anyhow!(e))
            }
            Ok(_msg_id) => {
                stats::MEMBERSHIP_PUBLISH_SUCCESS.inc();
                self.last_publish_timestamp = Timestamp::now();
                self.next_publish_timestamp =
                    self.last_publish_timestamp + self.publish_interval.period();
                self.publish_interval.reset(); // In case the change wasn't tiggered by the schedule.
                Ok(())
            }
        }
    }

    /// Publish the vote of the validator running the agent about a CID to a subnet.
    pub fn publish_vote(&mut self, vote: SignedVoteRecord<V>) -> anyhow::Result<()> {
        let topic = self.voting_topic(&vote.record().subnet_id);
        let data = vote.into_envelope().into_protobuf_encoding();
        match self.inner.publish(topic, data) {
            Err(e) => {
                stats::MEMBERSHIP_PUBLISH_FAILURE.inc();
                Err(anyhow!(e))
            }
            Ok(_msg_id) => {
                stats::MEMBERSHIP_PUBLISH_SUCCESS.inc();
                Ok(())
            }
        }
    }

    /// Publish arbitrary data to the pre-emptive topic of a subnet.
    ///
    /// We are not expected to be subscribed to this topic, only agents on the parent subnet are.
    pub fn publish_preemptive(&mut self, subnet_id: SubnetID, data: Vec<u8>) -> anyhow::Result<()> {
        let topic = self.preemptive_topic(&subnet_id);
        match self.inner.publish(topic, data) {
            Err(e) => {
                stats::MEMBERSHIP_PUBLISH_FAILURE.inc();
                Err(anyhow!(e))
            }
            Ok(_msg_id) => {
                stats::MEMBERSHIP_PUBLISH_SUCCESS.inc();
                Ok(())
            }
        }
    }

    /// Mark a peer as routable in the cache.
    ///
    /// Call this method when the discovery service learns the address of a peer.
    pub fn set_routable(&mut self, peer_id: PeerId) {
        self.provider_cache.set_routable(peer_id);
        stats::MEMBERSHIP_ROUTABLE_PEERS
            .set(self.provider_cache.num_routable().try_into().unwrap());
        self.publish_for_new_peer(peer_id);
    }

    /// Mark a peer as unroutable in the cache.
    ///
    /// Call this method when the discovery service forgets the address of a peer.
    pub fn set_unroutable(&mut self, peer_id: PeerId) {
        self.provider_cache.set_unroutable(peer_id);
        self.outbox.push_back(Event::Removed(peer_id))
    }

    /// List the current providers of a subnet.
    ///
    /// Call this method when looking for a peer to resolve content from.
    pub fn providers_of_subnet(&self, subnet_id: &SubnetID) -> Vec<PeerId> {
        self.provider_cache.providers_of_subnet(subnet_id)
    }

    /// Parse and handle a [`gossipsub::Message`]. If it's from the expected topic,
    /// then raise domain event to let the rest of the application know about a
    /// provider. Also update all the book keeping in the behaviour that we use
    /// to answer future queries about the topic.
    fn handle_message(&mut self, msg: gossipsub::Message) {
        if msg.topic == self.membership_topic.hash() {
            match SignedProviderRecord::from_bytes(&msg.data).map(|r| r.into_record()) {
                Ok(record) => self.handle_provider_record(record),
                Err(e) => {
                    stats::MEMBERSHIP_INVALID_MESSAGE.inc();
                    warn!(
                        "Gossip message from peer {:?} could not be deserialized as ProviderRecord: {e}",
                        msg.source
                    );
                }
            }
        } else if self.voting_topics.contains(&msg.topic) {
            match SignedVoteRecord::from_bytes(&msg.data).map(|r| r.into_record()) {
                Ok(record) => self.handle_vote_record(record),
                Err(e) => {
                    stats::MEMBERSHIP_INVALID_MESSAGE.inc();
                    warn!(
                        "Gossip message from peer {:?} could not be deserialized as VoteRecord: {e}",
                        msg.source
                    );
                }
            }
        } else if let Some(subnet_id) = self.preemptive_topics.get(&msg.topic) {
            self.handle_preemptive_data(subnet_id.clone(), msg.data)
        } else {
            stats::MEMBERSHIP_UNKNOWN_TOPIC.inc();
            warn!(
                "unknown gossipsub topic in message from {:?}: {}",
                msg.source, msg.topic
            );
        }
    }

    /// Try to add a provider record to the cache.
    ///
    /// If this is the first time we receive a record from the peer,
    /// reciprocate by publishing our own.
    fn handle_provider_record(&mut self, record: ProviderRecord) {
        debug!("received provider record: {record:?}");
        let (event, publish) = match self.provider_cache.add_provider(&record) {
            None => {
                stats::MEMBERSHIP_SKIPPED_PEERS.inc();
                (Some(Event::Skipped(record.peer_id)), false)
            }
            Some(d) if d.is_empty() && !d.is_new => (None, false),
            Some(d) => {
                let publish = d.is_new;
                (Some(Event::Updated(record.peer_id, d)), publish)
            }
        };

        if let Some(event) = event {
            self.outbox.push_back(event);
        }

        if publish {
            stats::MEMBERSHIP_PROVIDER_PEERS.inc();
            self.publish_for_new_peer(record.peer_id)
        }
    }

    /// Raise an event to tell we received a new vote.
    fn handle_vote_record(&mut self, record: VoteRecord<V>) {
        self.outbox.push_back(Event::ReceivedVote(Box::new(record)))
    }

    fn handle_preemptive_data(&mut self, subnet_id: SubnetID, data: Vec<u8>) {
        self.outbox
            .push_back(Event::ReceivedPreemptive(subnet_id, data))
    }

    /// Handle new subscribers to the membership topic.
    fn handle_subscriber(&mut self, peer_id: PeerId, topic: TopicHash) {
        if topic == self.membership_topic.hash() {
            self.publish_for_new_peer(peer_id)
        }
    }

    /// Publish our provider record when we encounter a new peer, unless we have recently done so.
    fn publish_for_new_peer(&mut self, peer_id: PeerId) {
        if self.subnet_ids.is_empty() {
            // We have nothing, so there's no need for them to know this ASAP.
            // The reason we shouldn't disable periodic publishing of empty
            // records completely is because it would also remove one of
            // triggers for non-connected peers to eagerly publish their
            // subnets when they see our empty records. Plus they could
            // be good to show on metrics, to have a single source of
            // the cluster size available on any node.
            return;
        }
        let now = Timestamp::now();
        if self.last_publish_timestamp > now - self.min_time_between_publish {
            debug!("recently published, not publishing again for peer {peer_id}");
        } else if self.next_publish_timestamp <= now + self.min_time_between_publish {
            debug!("publishing soon for new peer {peer_id}"); // don't let new joiners delay it forever by hitting the next block
        } else {
            debug!("publishing for new peer {peer_id}");
            // Create a new timer, rather than publish and reset. This way we don't repeat error handling.
            // Give some time for Kademlia and Identify to do their bit on both sides. Works better in tests.
            let delayed = Instant::now() + self.min_time_between_publish;
            self.next_publish_timestamp = now + self.min_time_between_publish;
            self.publish_interval =
                tokio::time::interval_at(delayed, self.publish_interval.period())
        }
    }

    /// Remove any membership record that hasn't been updated for a long time.
    fn prune_membership(&mut self) {
        let cutoff_timestamp = Timestamp::now() - self.max_provider_age;
        let pruned = self.provider_cache.prune_providers(cutoff_timestamp);
        for peer_id in pruned {
            stats::MEMBERSHIP_PROVIDER_PEERS.dec();
            self.outbox.push_back(Event::Removed(peer_id))
        }
    }
}

impl<V> NetworkBehaviour for Behaviour<V>
where
    V: Serialize + DeserializeOwned + Send + 'static,
{
    type ConnectionHandler = <gossipsub::Behaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event<V>;

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.inner.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // Emit own events first.
        if let Some(ev) = self.outbox.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(ev));
        }

        // Republish our current peer record snapshot and prune old records.
        if self.publish_interval.poll_tick(cx).is_ready() {
            if let Err(e) = self.publish_membership() {
                warn!("failed to publish membership: {e}")
            };
            self.prune_membership();
        }

        // Poll Gossipsub for events; this is where we can handle Gossipsub messages and
        // store the associations from peers to subnets.
        while let Poll::Ready(ev) = self.inner.poll(cx) {
            match ev {
                ToSwarm::GenerateEvent(ev) => {
                    match ev {
                        // NOTE: We could (ab)use the Gossipsub mechanism itself to signal subnet membership,
                        // however I think the information would only spread to our nearest neighbours we are
                        // connected to. If we assume there are hundreds of agents in each subnet which may
                        // or may not overlap, and each agent is connected to ~50 other agents, then the chance
                        // that there are subnets from which there are no or just a few connections is not
                        // insignificant. For this reason I opted to use messages instead, and let the content
                        // carry the information, spreading through the Gossipsub network regardless of the
                        // number of connected peers.
                        gossipsub::Event::Subscribed { peer_id, topic } => {
                            self.handle_subscriber(peer_id, topic)
                        }

                        gossipsub::Event::Unsubscribed { .. } => {}
                        // Log potential misconfiguration.
                        gossipsub::Event::GossipsubNotSupported { peer_id } => {
                            debug!("peer {peer_id} doesn't support gossipsub");
                        }
                        gossipsub::Event::Message { message, .. } => {
                            self.handle_message(message);
                        }
                    }
                }
                other => {
                    return Poll::Ready(other.map_out(|_| unreachable!("already handled")));
                }
            }
        }

        Poll::Pending
    }
}

// Forest has Filecoin specific values copied from Lotus. Not sure what values to use,
// so I'll leave everything on default for now. Or maybe they should be left empty?
mod scoring {

    use libp2p::gossipsub::{IdentTopic, PeerScoreParams, PeerScoreThresholds, TopicScoreParams};

    pub fn build_peer_score_params(membership_topic: IdentTopic) -> PeerScoreParams {
        let mut params = PeerScoreParams::default();
        params
            .topics
            .insert(membership_topic.hash(), TopicScoreParams::default());
        params
    }

    pub fn build_peer_score_thresholds() -> PeerScoreThresholds {
        PeerScoreThresholds::default()
    }
}
