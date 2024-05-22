// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use std::collections::{HashMap, HashSet};

use ipc_api::subnet_id::SubnetID;
use libp2p::PeerId;

use crate::{provider_record::ProviderRecord, Timestamp};

/// Change in the supported subnets of a peer.
#[derive(Debug)]
pub struct ProviderDelta {
    pub is_new: bool,
    pub added: Vec<SubnetID>,
    pub removed: Vec<SubnetID>,
}

impl ProviderDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Track which subnets are provided for by which set of peers.
pub struct SubnetProviderCache {
    /// Maximum number of subnets to track, to protect against DoS attacks, trying to
    /// flood someone with subnets that don't actually exist. When the number of subnets
    /// reaches this value, we remove the subnet with the smallest number of providers;
    /// hopefully this would be a subnet
    max_subnets: usize,
    /// User defined list of subnets which will never be pruned. This can be used to
    /// ward off attacks that would prevent us from adding subnets we know we want to
    /// support, and not rely on dynamic discovery of their peers.
    pinned_subnets: HashSet<SubnetID>,
    /// Set of peers with known addresses. Only such peers can be added to the cache.
    routable_peers: HashSet<PeerId>,
    /// List of peer IDs supporting each subnet.
    subnet_providers: HashMap<SubnetID, HashSet<PeerId>>,
    /// Timestamp of the last record received about a peer.
    peer_timestamps: HashMap<PeerId, Timestamp>,
}

impl SubnetProviderCache {
    pub fn new(max_subnets: usize, static_subnets: Vec<SubnetID>) -> Self {
        Self {
            pinned_subnets: HashSet::from_iter(static_subnets),
            max_subnets,
            routable_peers: Default::default(),
            subnet_providers: Default::default(),
            peer_timestamps: Default::default(),
        }
    }

    /// Pin a subnet, after which it won't be pruned.
    pub fn pin_subnet(&mut self, subnet_id: SubnetID) {
        self.pinned_subnets.insert(subnet_id);
    }

    /// Unpin a subnet, which allows it to be pruned.
    pub fn unpin_subnet(&mut self, subnet_id: &SubnetID) {
        self.pinned_subnets.remove(subnet_id);
    }

    /// Mark a peer as routable.
    ///
    /// Once routable, the cache will keep track of provided subnets.
    pub fn set_routable(&mut self, peer_id: PeerId) {
        self.routable_peers.insert(peer_id);
    }

    /// Mark a previously routable peer as unroutable.
    ///
    /// Once unroutable, the cache will stop tracking the provided subnets.
    pub fn set_unroutable(&mut self, peer_id: PeerId) {
        self.routable_peers.remove(&peer_id);
        self.peer_timestamps.remove(&peer_id);
        for providers in self.subnet_providers.values_mut() {
            providers.remove(&peer_id);
        }
    }

    /// Number of routable peers.
    pub fn num_routable(&mut self) -> usize {
        self.routable_peers.len()
    }

    /// Check if a peer has been marked as routable.
    pub fn is_routable(&self, peer_id: &PeerId) -> bool {
        self.routable_peers.contains(peer_id)
    }

    /// Check whether we have received recent updates from a peer.
    pub fn has_timestamp(&self, peer_id: &PeerId) -> bool {
        self.peer_timestamps.contains_key(peer_id)
    }

    /// Try to add a provider to the cache.
    ///
    /// Returns `None` if the peer is not routable and nothing could be added.
    ///
    /// Returns `Some` if the peer is routable, containing the newly added
    /// and newly removed associations for this peer.
    pub fn add_provider(&mut self, record: &ProviderRecord) -> Option<ProviderDelta> {
        if !self.is_routable(&record.peer_id) {
            return None;
        }

        let mut delta = ProviderDelta {
            is_new: !self.has_timestamp(&record.peer_id),
            added: Vec::new(),
            removed: Vec::new(),
        };

        let timestamp = self.peer_timestamps.entry(record.peer_id).or_default();

        if *timestamp < record.timestamp {
            *timestamp = record.timestamp;

            // The currently supported subnets of the peer.
            let mut subnet_ids = HashSet::new();
            subnet_ids.extend(record.subnet_ids.iter());

            // Remove the peer from subnets it no longer supports.
            for (subnet_id, peer_ids) in self.subnet_providers.iter_mut() {
                if !subnet_ids.contains(subnet_id) && peer_ids.remove(&record.peer_id) {
                    delta.removed.push(subnet_id.clone());
                }
            }

            // Add peer to new subnets it supports now.
            for subnet_id in record.subnet_ids.iter() {
                let peer_ids = self.subnet_providers.entry(subnet_id.clone()).or_default();
                if peer_ids.insert(record.peer_id) {
                    delta.added.push(subnet_id.clone());
                }
            }

            // Remove subnets that have been added but are too small to survive a pruning.
            let removed_subnet_ids = self.prune_subnets();
            delta.added.retain(|id| !removed_subnet_ids.contains(id))
        }

        Some(delta)
    }

    /// Ensure we don't have more than `max_subnets` number of subnets in the cache.
    ///
    /// Returns the removed subnet IDs.
    fn prune_subnets(&mut self) -> HashSet<SubnetID> {
        let mut removed_subnet_ids = HashSet::new();

        let to_prune = self.subnet_providers.len().saturating_sub(self.max_subnets);
        if to_prune > 0 {
            let mut counts = self
                .subnet_providers
                .iter()
                .map(|(id, ps)| (id.clone(), ps.len()))
                .collect::<Vec<_>>();

            counts.sort_by_key(|(_, count)| *count);

            for (subnet_id, _) in counts {
                if self.pinned_subnets.contains(&subnet_id) {
                    continue;
                }
                self.subnet_providers.remove(&subnet_id);
                removed_subnet_ids.insert(subnet_id);
                if removed_subnet_ids.len() == to_prune {
                    break;
                }
            }
        }

        removed_subnet_ids
    }

    /// Prune any provider which hasn't provided an update since a cutoff timestamp.
    ///
    /// Returns the list of pruned peers.
    pub fn prune_providers(&mut self, cutoff_timestamp: Timestamp) -> Vec<PeerId> {
        let to_prune = self
            .peer_timestamps
            .iter()
            .filter_map(|(id, ts)| {
                if *ts < cutoff_timestamp {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for peer_id in to_prune.iter() {
            self.set_unroutable(*peer_id);
        }

        to_prune
    }

    /// List any known providers of a subnet.
    pub fn providers_of_subnet(&self, subnet_id: &SubnetID) -> Vec<PeerId> {
        self.subnet_providers
            .get(subnet_id)
            .map(|hs| hs.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use ipc_api::subnet_id::SubnetID;
    use libp2p::{identity::Keypair, PeerId};
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    use crate::{arb::ArbSubnetID, provider_record::ProviderRecord, Timestamp};

    use super::SubnetProviderCache;

    #[derive(Debug, Clone)]
    struct TestRecords(Vec<ProviderRecord>);

    // Limited number of records from a limited set of peers.
    impl Arbitrary for TestRecords {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let rc = usize::arbitrary(g) % 20;
            let pc = 1 + rc / 2;

            let mut ps = Vec::new();
            let mut rs = Vec::new();

            for _ in 0..pc {
                let pk = Keypair::generate_ed25519();
                let peer_id = pk.public().to_peer_id();
                ps.push(peer_id)
            }

            for _ in 0..rc {
                let peer_id = ps[usize::arbitrary(g) % ps.len()];
                let mut subnet_ids = Vec::new();
                for _ in 0..usize::arbitrary(g) % 5 {
                    subnet_ids.push(ArbSubnetID::arbitrary(g).0)
                }
                let record = ProviderRecord {
                    peer_id,
                    subnet_ids,
                    timestamp: Timestamp::arbitrary(g),
                };
                rs.push(record)
            }

            Self(rs)
        }
    }

    type Providers = HashMap<SubnetID, HashSet<PeerId>>;

    /// Build a provider mapping to check the cache against.
    fn build_providers(records: &Vec<ProviderRecord>) -> Providers {
        // Only the last timestamp should be kept, but it might not be unique.
        let mut max_timestamps: HashMap<PeerId, Timestamp> = Default::default();
        for record in records {
            let mts = max_timestamps.entry(record.peer_id).or_default();
            if *mts < record.timestamp {
                *mts = record.timestamp;
            }
        }

        let mut providers: HashMap<SubnetID, HashSet<PeerId>> = Default::default();
        let mut seen: HashSet<PeerId> = Default::default();

        for record in records {
            if record.timestamp != max_timestamps[&record.peer_id] {
                continue;
            }
            if !seen.insert(record.peer_id) {
                continue;
            }
            for subnet_id in record.subnet_ids.iter() {
                providers
                    .entry(subnet_id.clone())
                    .or_default()
                    .insert(record.peer_id);
            }
        }

        providers
    }

    /// Check the cache against the reference built in the test.
    fn check_providers(providers: &Providers, cache: &SubnetProviderCache) -> Result<(), String> {
        for (subnet_id, exp_peer_ids) in providers {
            let peer_ids = cache.providers_of_subnet(subnet_id);
            if peer_ids.len() != exp_peer_ids.len() {
                return Err(format!(
                    "expected {} peers, got {} in subnet {:?}",
                    exp_peer_ids.len(),
                    peer_ids.len(),
                    subnet_id
                ));
            }
            for peer_id in peer_ids {
                if !exp_peer_ids.contains(&peer_id) {
                    return Err("wrong peer ID".into());
                }
            }
        }
        Ok(())
    }

    #[quickcheck]
    fn prop_subnets_pruned(records: TestRecords, max_subnets: usize) -> bool {
        let max_subnets = max_subnets % 10;
        let mut cache = SubnetProviderCache::new(max_subnets, Vec::new());
        for record in records.0 {
            cache.set_routable(record.peer_id);
            if cache.add_provider(&record).is_none() {
                return false;
            }
        }
        cache.subnet_providers.len() <= max_subnets
    }

    #[quickcheck]
    fn prop_subnets_pinned(records: TestRecords) -> Result<(), String> {
        // Find two subnets to pin.
        let providers = build_providers(&records.0);
        if providers.len() < 2 {
            return Ok(());
        }

        let subnets = providers.keys().take(2).collect::<Vec<_>>();

        let mut cache = SubnetProviderCache::new(3, vec![subnets[0].clone()]);
        cache.pin_subnet(subnets[1].clone());

        for record in records.0 {
            cache.set_routable(record.peer_id);
            cache.add_provider(&record);
        }

        if !cache.subnet_providers.contains_key(subnets[0]) {
            return Err("static subnet not found".into());
        }
        if !cache.subnet_providers.contains_key(subnets[1]) {
            return Err("pinned subnet not found".into());
        }
        Ok(())
    }

    #[quickcheck]
    fn prop_providers_listed(records: TestRecords) -> Result<(), String> {
        let records = records.0;
        let mut cache = SubnetProviderCache::new(usize::MAX, Vec::new());

        for record in records.iter() {
            cache.set_routable(record.peer_id);
            cache.add_provider(record);
        }

        let providers = build_providers(&records);

        check_providers(&providers, &cache)
    }

    #[quickcheck]
    fn prop_providers_pruned(
        records: TestRecords,
        cutoff_timestamp: Timestamp,
    ) -> Result<(), String> {
        let mut records = records.0;
        let mut cache = SubnetProviderCache::new(usize::MAX, Vec::new());
        for record in records.iter() {
            cache.set_routable(record.peer_id);
            cache.add_provider(record);
        }
        cache.prune_providers(cutoff_timestamp);

        // Build a reference from only what has come after the cutoff timestamp.
        records.retain(|r| r.timestamp >= cutoff_timestamp);

        let providers = build_providers(&records);

        check_providers(&providers, &cache)
    }
}
