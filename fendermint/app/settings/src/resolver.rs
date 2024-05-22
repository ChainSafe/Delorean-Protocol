// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, time::Duration};

use serde::Deserialize;
use serde_with::{serde_as, DurationSeconds};

use ipc_api::subnet_id::SubnetID;
use multiaddr::Multiaddr;

use crate::{home_relative, IsHumanReadable};

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct ResolverSettings {
    /// Time to wait between attempts to resolve a CID, in seconds.
    #[serde_as(as = "DurationSeconds<u64>")]
    pub retry_delay: Duration,
    pub network: NetworkSettings,
    pub discovery: DiscoverySettings,
    pub membership: MembershipSettings,
    pub connection: ConnectionSettings,
    pub content: ContentSettings,
}

/// Settings describing the subnet hierarchy, not the physical network.
///
/// For physical network settings see [ConnectionSettings].
#[derive(Clone, Debug, Deserialize)]
pub struct NetworkSettings {
    /// Cryptographic key used to sign messages.
    ///
    /// This is the name of a Secp256k1 private key file,
    /// relative to the `home_dir`.
    local_key: PathBuf,
    /// Network name to differentiate this peer group.
    pub network_name: String,
}

home_relative!(NetworkSettings { local_key });

/// Configuration for [`discovery::Behaviour`].
#[derive(Clone, Debug, Deserialize)]
pub struct DiscoverySettings {
    /// Custom nodes which never expire, e.g. bootstrap or reserved nodes.
    ///
    /// The addresses must end with a `/p2p/<peer-id>` part.
    pub static_addresses: Vec<Multiaddr>,
    /// Number of connections at which point we pause further discovery lookups.
    pub target_connections: usize,
    /// Option to disable Kademlia, for example in a fixed static network.
    pub enable_kademlia: bool,
}

/// Configuration for [`membership::Behaviour`].
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
pub struct MembershipSettings {
    /// User defined list of subnets which will never be pruned from the cache.
    #[serde_as(as = "Vec<IsHumanReadable>")]
    pub static_subnets: Vec<SubnetID>,

    /// Maximum number of subnets to track in the cache.
    pub max_subnets: usize,

    /// Publish interval for supported subnets.
    #[serde_as(as = "DurationSeconds<u64>")]
    pub publish_interval: Duration,

    /// Minimum time between publishing own provider record in reaction to new joiners.
    #[serde_as(as = "DurationSeconds<u64>")]
    pub min_time_between_publish: Duration,

    /// Maximum age of provider records before the peer is removed without an update.
    #[serde_as(as = "DurationSeconds<u64>")]
    pub max_provider_age: Duration,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionSettings {
    /// The address where we will listen to incoming connections.
    pub listen_addr: Multiaddr,
    /// A list of known external addresses this node is reachable on.
    pub external_addresses: Vec<Multiaddr>,
    /// Maximum number of incoming connections.
    pub max_incoming: u32,
    /// Expected number of peers, for sizing the Bloom filter.
    pub expected_peer_count: u32,
    /// Maximum number of peers to send Bitswap requests to in a single attempt.
    pub max_peers_per_query: u32,
    /// Maximum number of events in the push-based broadcast channel before a slow
    /// consumer gets an error because it's falling behind.
    pub event_buffer_capacity: u32,
}

/// Configuration for [`content::Behaviour`].
#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct ContentSettings {
    /// Number of bytes that can be consumed by remote peers in a time period.
    ///
    /// 0 means no limit.
    pub rate_limit_bytes: u32,
    /// Length of the time period at which the consumption limit fills.
    ///
    /// 0 means no limit.
    #[serde_as(as = "DurationSeconds<u64>")]
    pub rate_limit_period: Duration,
}
