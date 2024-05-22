// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use std::{
    collections::{HashMap, VecDeque},
    task::{Context, Poll},
    time::Duration,
};

use libipld::{store::StoreParams, Cid};
use libp2p::{
    core::{ConnectedPoint, Endpoint},
    futures::channel::oneshot,
    multiaddr::Protocol,
    swarm::{
        derive_prelude::FromSwarm, ConnectionDenied, ConnectionId, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use libp2p_bitswap::{Bitswap, BitswapConfig, BitswapEvent, BitswapResponse, BitswapStore};
use log::debug;
use prometheus::Registry;

use crate::{
    limiter::{RateLimit, RateLimiter},
    stats,
};

pub type QueryId = libp2p_bitswap::QueryId;

// Not much to do here, just hiding the `Progress` event as I don't think we'll need it.
// We can't really turn it into anything more meaningful; the outer Service, which drives
// the Swarm events, will have to store the `QueryId` and figure out which CID it was about
// (there could be multiple queries running over the same CID) and how to respond to the
// original requestor (e.g. by completing a channel).
#[derive(Debug)]
pub enum Event {
    /// Event raised when a resolution request is finished.
    ///
    /// The result will indicate either success, or arbitrary failure.
    /// If it is a success, the CID can be found in the [`BitswapStore`]
    /// instance the behaviour was created with.
    ///
    /// Note that it is possible that the synchronization completed
    /// partially, but some recursive constituent is missing. The
    /// caller can use the [`missing_blocks`] function to check
    /// whether a retry is necessary.
    Complete(QueryId, anyhow::Result<()>),

    /// Event raised when we want to execute some logic with the `BitswapResponse`.
    /// This is only raised if we are tracking rate limits. The service has to
    /// do the forwarding between the two oneshot channels, and call this module
    /// back between doing so.
    #[allow(dead_code)]
    BitswapForward {
        peer_id: PeerId,
        /// Receive response from the [`Bitswap`] behaviour.
        /// Normally this goes straight to the handler.
        response_rx: oneshot::Receiver<BitswapResponse>,
        /// Forward the response to the handler.
        response_tx: oneshot::Sender<BitswapResponse>,
    },
}

/// Configuration for [`content::Behaviour`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Number of bytes that can be consumed remote peers in a time period.
    ///
    /// 0 means no limit.
    pub rate_limit_bytes: u32,
    /// Length of the time period at which the consumption limit fills.
    ///
    /// 0 means no limit.
    pub rate_limit_period: Duration,
}

/// Behaviour built on [`Bitswap`] to resolve IPLD content from [`Cid`] to raw bytes.
pub struct Behaviour<P: StoreParams> {
    inner: Bitswap<P>,
    /// Remember which address peers connected from, so we can apply the rate limit
    /// on the address, and not on the peer ID which they can change easily.
    peer_addresses: HashMap<PeerId, Multiaddr>,
    /// Limit the amount of data served by remote address.
    rate_limiter: RateLimiter<Multiaddr>,
    rate_limit_period: Duration,
    rate_limit: Option<RateLimit>,
    outbox: VecDeque<Event>,
}

impl<P: StoreParams> Behaviour<P> {
    pub fn new<S>(config: Config, store: S) -> Self
    where
        S: BitswapStore<Params = P>,
    {
        let bitswap = Bitswap::new(BitswapConfig::default(), store);
        let rate_limit = if config.rate_limit_bytes == 0 || config.rate_limit_period.is_zero() {
            None
        } else {
            Some(RateLimit::new(
                config.rate_limit_bytes,
                config.rate_limit_period,
            ))
        };
        Self {
            inner: bitswap,
            peer_addresses: Default::default(),
            rate_limiter: RateLimiter::new(config.rate_limit_period),
            rate_limit_period: config.rate_limit_period,
            rate_limit,
            outbox: Default::default(),
        }
    }

    /// Register Prometheus metrics.
    pub fn register_metrics(&self, registry: &Registry) -> anyhow::Result<()> {
        self.inner.register_metrics(registry)
    }

    /// Recursively resolve a [`Cid`] and all underlying CIDs into blocks.
    ///
    /// The [`Bitswap`] behaviour will call the [`BitswapStore`] to ask for
    /// blocks which are missing, ie. find CIDs which aren't available locally.
    /// It is up to the store implementation to decide which links need to be
    /// followed.
    ///
    /// It is also up to the store implementation to decide which CIDs requests
    /// to responds to, e.g. if we only want to resolve certain type of content,
    /// then the store can look up in a restricted collection, rather than the
    /// full IPLD store.
    ///
    /// Resolution will be attempted from the peers passed to the method,
    /// starting with the first one with `WANT-BLOCK`, then whoever responds
    /// positively to `WANT-HAVE` requests. The caller should talk to the
    /// `membership::Behaviour` first to find suitable peers, and then
    /// prioritise peers which are connected.
    ///
    /// The underlying [`libp2p_request_response::RequestResponse`] behaviour
    /// will initiate connections to the peers which aren't connected at the moment.
    pub fn resolve(&mut self, cid: Cid, peers: Vec<PeerId>) -> QueryId {
        debug!("resolving {cid} from {peers:?}");
        stats::CONTENT_RESOLVE_RUNNING.inc();
        // Not passing any missing items, which will result in a call to `BitswapStore::missing_blocks`.
        self.inner.sync(cid, peers, [].into_iter())
    }

    /// Check whether the peer has already exhaused their rate limit.
    #[allow(dead_code)]
    fn check_rate_limit(&mut self, peer_id: &PeerId, cid: &Cid) -> bool {
        if let Some(ref rate_limit) = self.rate_limit {
            if let Some(addr) = self.peer_addresses.get(peer_id).cloned() {
                let bytes = cid.to_bytes().len().try_into().unwrap_or(u32::MAX);

                if !self.rate_limiter.add(rate_limit, addr, bytes) {
                    return false;
                }
            }
        }
        true
    }

    /// Callback by the service after [`Event::BitswapForward`].
    pub fn rate_limit_used(&mut self, peer_id: PeerId, bytes: usize) {
        if let Some(ref rate_limit) = self.rate_limit {
            if let Some(addr) = self.peer_addresses.get(&peer_id).cloned() {
                let bytes = bytes.try_into().unwrap_or(u32::MAX);
                let _ = self.rate_limiter.add(rate_limit, addr, bytes);
            }
        }
    }

    /// Update the rate limit to a new value, keeping the period as-is.
    pub fn update_rate_limit(&mut self, bytes: u32) {
        if bytes == 0 || self.rate_limit_period.is_zero() {
            self.rate_limit = None;
        } else {
            self.rate_limit = Some(RateLimit::new(bytes, self.rate_limit_period))
        }
    }
}

impl<P: StoreParams> NetworkBehaviour for Behaviour<P> {
    type ConnectionHandler = <Bitswap<P> as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event;

    fn on_swarm_event(&mut self, event: FromSwarm) {
        // Store the remote address.
        match &event {
            FromSwarm::ConnectionEstablished(c) => {
                if c.other_established == 0 {
                    let peer_addr = match c.endpoint {
                        ConnectedPoint::Dialer {
                            address: listen_addr,
                            ..
                        } => listen_addr.clone(),
                        ConnectedPoint::Listener {
                            send_back_addr: ephemeral_addr,
                            ..
                        } => select_non_ephemeral(ephemeral_addr.clone()),
                    };
                    self.peer_addresses.insert(c.peer_id, peer_addr);
                }
            }
            FromSwarm::ConnectionClosed(c) => {
                if c.remaining_established == 0 {
                    self.peer_addresses.remove(&c.peer_id);
                }
            }
            // Note: Ignoring FromSwarm::AddressChange - as long as the same peer connects,
            // not updating the address provides continuity of resource consumption.
            _ => {}
        }

        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        // TODO: `request_response::handler` is now private, so we cannot pattern match on the handler event.
        // By the looks of the only way to access the request event is to let it go right into the RR protocol
        // wrapped by the Bitswap behaviour and let it raise an event, however we will not see that event here.
        // I'm not sure what we can do without moving rate limiting into the bitswap library itself, because
        // what we did here relied on the ability to redirect the channels inside the request, but if the event
        // itself is private to the `request_response` protocol there's nothing I can do.
        // match event {

        //     request_response::handler::Event::Request {
        //         request_id,
        //         request,
        //         sender,
        //     } if self.rate_limit.is_some() => {
        //         if !self.check_rate_limit(&peer_id, &request.cid) {
        //             warn!("rate limiting {peer_id}");
        //             stats::CONTENT_RATE_LIMITED.inc();
        //             return;
        //         }
        //         // We need to hijack the response channel to record the size, otherwise it goes straight to the handler.
        //         let (tx, rx) = libp2p::futures::channel::oneshot::channel();
        //         let event = request_response::Event::Request {
        //             request_id,
        //             request,
        //             sender: tx,
        //         };

        //         self.inner
        //             .on_connection_handler_event(peer_id, connection_id, event);

        //         let forward = Event::BitswapForward {
        //             peer_id,
        //             response_rx: rx,
        //             response_tx: sender,
        //         };
        //         self.outbox.push_back(forward);
        //     }
        //     _ => self
        //         .inner
        //         .on_connection_handler_event(peer_id, connection_id, event),
        // }

        // debug!("BITSWAP CONNECTION HANDLER EVENT: {event:?}");

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
        // Poll Bitswap.
        while let Poll::Ready(ev) = self.inner.poll(cx) {
            // debug!("BITSWAP POLL: {ev:?}");
            match ev {
                ToSwarm::GenerateEvent(ev) => match ev {
                    BitswapEvent::Progress(_, _) => {}
                    BitswapEvent::Complete(id, result) => {
                        stats::CONTENT_RESOLVE_RUNNING.dec();
                        let out = Event::Complete(id, result);
                        return Poll::Ready(ToSwarm::GenerateEvent(out));
                    }
                },
                other => {
                    return Poll::Ready(other.map_out(|_| unreachable!("already handled")));
                }
            }
        }

        Poll::Pending
    }
}

/// Get rid of parts of an address which are considered ephemeral,
/// keeping just the parts which would stay the same if for example
/// the same peer opened another connection from a different random port.
fn select_non_ephemeral(mut addr: Multiaddr) -> Multiaddr {
    let mut keep = Vec::new();
    while let Some(proto) = addr.pop() {
        match proto {
            // Some are valid on their own right.
            Protocol::Ip4(_) | Protocol::Ip6(_) => {
                keep.clear();
                keep.push(proto);
                break;
            }
            // Skip P2P peer ID, they might use a different identity.
            Protocol::P2p(_) => {}
            // Skip ephemeral parts.
            Protocol::Tcp(_) | Protocol::Udp(_) => {}
            // Everything else we keep until we see better options.
            _ => {
                keep.push(proto);
            }
        }
    }
    keep.reverse();
    Multiaddr::from_iter(keep)
}

#[cfg(test)]
mod tests {
    use libp2p::Multiaddr;

    use super::select_non_ephemeral;

    #[test]
    fn non_ephemeral_addr() {
        let examples = [
            ("/ip4/127.0.0.1/udt/sctp/5678", "/ip4/127.0.0.1"),
            ("/ip4/95.217.194.97/tcp/8008/p2p/12D3KooWC1EaEEpghwnPdd89LaPTKEweD1PRLz4aRBkJEA9UiUuS", "/ip4/95.217.194.97"),
            ("/udt/memory/10/p2p/12D3KooWC1EaEEpghwnPdd89LaPTKEweD1PRLz4aRBkJEA9UiUuS", "/udt/memory/10")
        ];

        for (addr, exp) in examples {
            let addr: Multiaddr = addr.parse().unwrap();
            let exp: Multiaddr = exp.parse().unwrap();
            assert_eq!(select_non_ephemeral(addr), exp);
        }
    }
}
