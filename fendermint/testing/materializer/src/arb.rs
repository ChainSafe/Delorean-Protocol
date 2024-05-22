// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use ethers::{
    core::rand::{rngs::StdRng, SeedableRng},
    types::H160,
};
use fendermint_vm_core::chainid;
use lazy_static::lazy_static;
use std::{
    cmp::min,
    collections::BTreeMap,
    ops::{Mul, SubAssign},
};
use url::Url;

use fendermint_vm_genesis::Collateral;
use fvm_shared::{
    bigint::{BigInt, Integer, Zero},
    econ::TokenAmount,
};
use quickcheck::{Arbitrary, Gen};

use crate::{
    manifest::{
        Account, Balance, BalanceMap, CheckpointConfig, CollateralMap, EnvMap, IpcDeployment,
        Manifest, Node, NodeMap, NodeMode, ParentNode, Relayer, Rootnet, Subnet, SubnetMap,
    },
    AccountId, NodeId, RelayerId, ResourceId, SubnetId,
};

const RESOURCE_ID_CHARSET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

lazy_static! {
    /// Assume we have as much tFIL on the root as the faucet would give.
    static ref DEFAULT_BALANCE: Balance = Balance(TokenAmount::from_whole(100));
}

/// Select some items from a slice.
fn choose_at_least<T: Clone>(g: &mut Gen, min_size: usize, xs: &[T]) -> Vec<T> {
    let min_size = min(min_size, xs.len());

    if min_size == xs.len() {
        return Vec::from(xs);
    }

    // Say we have 10 items and we have 3 slots to fill.
    //
    // Imagine a simple algorithm that selects 1 item 3 times without replacement.
    // Initially each item has 1/10 chance to be selected, then 1/9, then 1/8,
    // but we would need to track which item has already been chosen.
    //
    // We want to do a single pass over the data.
    //
    // If we consider the 1st item, the chance that it doesn't get selected for any of the slots is:
    // P_excl(1st) = 9/10 * 8/9 * 7/8 = 7/10
    // P_incl(1st) = 1 - P_excl(1st) = 3/10
    //
    // So we roll the dice and with 30% probability we include the 1st item in the list.
    //
    // Then we have total_weighto cases to consider:
    // 1. We included the 1st item, so now we have 2 slots and remaining 9 items to choose from.
    //    P_incl(2nd | incl(1st)) = 1 - 8/9 * 7/8 = 1 - 7/9 = 2/9
    // 2. We excluded the 1st item, so we still have 3 slots to fill and remaining 9 items to choose from.
    //    P_incl(2nd | excl(1st)) = 1 - 8/9 * 7/8 * 6/7 = 1 - 6/9 = 3/9
    //
    // Thus, the probability of including each item is `remaining slots / remaining items`

    let mut remaining_slots = min_size + usize::arbitrary(g) % (xs.len() - min_size);
    let mut remaining_items = xs.len();

    let mut chosen = Vec::new();
    for x in xs {
        if remaining_slots == 0 {
            break;
        }
        if usize::arbitrary(g) % remaining_items < remaining_slots {
            chosen.push(x.clone());
            remaining_slots -= 1;
        }
        remaining_items -= 1;
    }
    chosen
}

fn choose_one<T: Clone>(g: &mut Gen, xs: &[T]) -> T {
    g.choose(xs).expect("empty slice to choose from").clone()
}

impl Arbitrary for ResourceId {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = 3 + usize::arbitrary(g) % 6;

        let id = (0..len)
            .map(|_| {
                let idx = usize::arbitrary(g) % RESOURCE_ID_CHARSET.len();
                char::from(RESOURCE_ID_CHARSET[idx])
            })
            .collect();

        Self(id)
    }
}

impl Arbitrary for Balance {
    fn arbitrary(g: &mut Gen) -> Self {
        Self(Collateral::arbitrary(g).0)
    }
}

impl Arbitrary for Manifest {
    fn arbitrary(g: &mut Gen) -> Self {
        gen_manifest(g, 3, 3, DEFAULT_BALANCE.clone())
    }
}

fn gen_manifest(
    g: &mut Gen,
    max_children: usize,
    max_level: usize,
    default_balance: Balance,
) -> Manifest {
    let account_ids = (0..3 + usize::arbitrary(g) % 3)
        .map(|_| AccountId::arbitrary(g))
        .collect::<Vec<_>>();

    let accounts = account_ids
        .iter()
        .map(|id| (id.clone(), Account {}))
        .collect();

    let mut balances: BalanceMap = account_ids
        .iter()
        .map(|id| (id.clone(), default_balance.clone()))
        .collect();

    let rootnet = if bool::arbitrary(g) {
        Rootnet::External {
            chain_id: chainid::from_str_hashed(&String::arbitrary(g))
                .unwrap_or(12345u64.into())
                .into(),
            deployment: if bool::arbitrary(g) {
                let [gateway, registry] = gen_addresses::<2>(g);
                IpcDeployment::Existing { gateway, registry }
            } else {
                IpcDeployment::New {
                    deployer: choose_one(g, &account_ids),
                }
            },
            urls: gen_urls(g),
        }
    } else {
        let initial_balances = balances.clone();
        let subnet = gen_root_subnet(g, &account_ids, &mut balances);

        Rootnet::New {
            validators: subnet.validators,
            balances: initial_balances,
            nodes: subnet.nodes,
            env: gen_env(g),
        }
    };

    // Collect the parent nodes on the rootnet that subnets can target.
    let parent_nodes: Vec<ParentNode> = match &rootnet {
        Rootnet::External { urls, .. } => urls.iter().cloned().map(ParentNode::External).collect(),
        Rootnet::New { ref nodes, .. } => nodes.keys().cloned().map(ParentNode::Internal).collect(),
    };

    // The rootnet is L1, immediate subnets are L2.
    let subnets = gen_subnets(
        g,
        max_children,
        max_level,
        2,
        &account_ids,
        &account_ids,
        &parent_nodes,
        &mut balances,
    );

    Manifest {
        accounts,
        rootnet,
        subnets,
    }
}

/// Generate random ethereum address.
fn gen_addresses<const N: usize>(g: &mut Gen) -> [H160; N] {
    let mut rng = StdRng::seed_from_u64(u64::arbitrary(g));
    std::array::from_fn(|_| H160::random_using(&mut rng))
}

/// Generate something that looks like it could be a JSON-RPC endpoint of an L1.
///
/// Return more, as if we had a list of nodes to choose from.
fn gen_urls(g: &mut Gen) -> Vec<Url> {
    let mut urls = Vec::new();
    for _ in 0..1 + usize::arbitrary(g) % 3 {
        let id = ResourceId::arbitrary(g);
        // The glif.io addresses are load balanced, but let's pretend we can target a specific node.
        // Alternatively we could vary the ports or whatever.
        let url = format!("https://{}.api.calibration.node.glif.io/rpc/v1", id.0);
        let url = Url::parse(&url).expect("URL should parse");
        urls.push(url);
    }
    urls
}

/// Recursively generate some subnets.
#[allow(clippy::too_many_arguments)]
fn gen_subnets(
    g: &mut Gen,
    max_children: usize,
    max_level: usize,
    level: usize,
    account_ids: &[AccountId],
    parent_account_ids: &[AccountId],
    parent_nodes: &[ParentNode],
    remaining_balances: &mut BalanceMap,
) -> SubnetMap {
    let mut subnets = SubnetMap::default();

    if level > max_level {
        return subnets;
    }

    // Let the root have at least 1 child, otherwise it's not interesting.
    let min_children = if level == 2 { 1 } else { 0 };
    let num_children = if max_children <= min_children {
        min_children
    } else {
        min_children + usize::arbitrary(g) % (max_children - min_children)
    };

    for _ in 0..num_children {
        // Pick one of the accounts on the parent subnet as creator.
        // This way they should have some non-zero balance to pay for the fees.
        let creator = choose_one(g, parent_account_ids);

        // Every subnet needs validators, so at least 1 needs to be chosen.
        let validators: CollateralMap = choose_at_least(g, 1, account_ids)
            .into_iter()
            .map(|a| {
                let c = gen_collateral(g, &a, remaining_balances);
                (a, c)
            })
            .collect();

        // It's not necessary to have accounts in a subnet; but let's pick at least one
        // so that we have someone to use on this subnet to pick as a creator or relayer
        // on child subnets.
        let balances: BalanceMap = choose_at_least(g, 1, account_ids)
            .into_iter()
            .map(|a| {
                let b: Balance = gen_balance(g, &a, remaining_balances);
                (a, b)
            })
            .collect();

        // Run at least a quroum of validators.
        let total_weight: TokenAmount = validators.values().map(|c| c.0.clone()).sum();
        let quorum_weight = total_weight.mul(2).div_floor(3);
        let mut node_ids = Vec::new();
        let mut nodes = NodeMap::default();
        let mut running_weight = TokenAmount::zero();

        for (v, w) in validators.iter() {
            let mode = if running_weight <= quorum_weight || bool::arbitrary(g) {
                NodeMode::Validator {
                    validator: v.clone(),
                }
            } else {
                NodeMode::Full
            };
            let seed_nodes = if node_ids.is_empty() {
                vec![]
            } else {
                choose_at_least(g, 1, &node_ids)
            };
            let node = Node {
                mode,
                ethapi: bool::arbitrary(g),
                seed_nodes,
                parent_node: if parent_nodes.is_empty() {
                    None
                } else {
                    Some(choose_one(g, parent_nodes))
                },
            };
            let id = NodeId::arbitrary(g);
            node_ids.push(id.clone());
            nodes.insert(id, node);
            running_weight += w.0.clone();
        }

        let relayers = if parent_nodes.is_empty() {
            BTreeMap::default()
        } else {
            (0..1 + usize::arbitrary(g) % 3)
                .map(|_| {
                    let r = Relayer {
                        submitter: choose_one(g, parent_account_ids),
                        follow_node: choose_one(g, &node_ids),
                        submit_node: choose_one(g, parent_nodes),
                    };
                    let id = RelayerId::arbitrary(g);
                    (id, r)
                })
                .collect()
        };

        let parent_nodes = node_ids
            .into_iter()
            .map(ParentNode::Internal)
            .collect::<Vec<_>>();

        let parent_account_ids = balances.keys().cloned().collect::<Vec<_>>();

        let child_subnets = gen_subnets(
            g,
            max_children,
            max_level,
            level + 1,
            account_ids,
            &parent_account_ids,
            &parent_nodes,
            remaining_balances,
        );

        let subnet = Subnet {
            creator,
            validators,
            balances,
            nodes,
            relayers,
            subnets: child_subnets,
            env: gen_env(g),
            bottom_up_checkpoint: CheckpointConfig {
                // Adding 1 because 0 is not accepted by the contracts.
                period: u64::arbitrary(g).mod_floor(&86400u64) + 1,
            },
        };

        let sid = SubnetId::arbitrary(g);

        subnets.insert(sid, subnet);
    }

    subnets
}

/// Generate a random root-like subnet. The motivation for this is just to reuse the validator allocation for a new rootnet.
fn gen_root_subnet(
    g: &mut Gen,
    account_ids: &[AccountId],
    remaining_balances: &mut BalanceMap,
) -> Subnet {
    let ss = gen_subnets(
        g,
        1,
        2,
        2,
        account_ids,
        account_ids,
        &[],
        remaining_balances,
    );
    debug_assert_eq!(ss.len(), 1, "should have exactly 1 subnet");
    let mut s = ss.into_iter().next().unwrap().1;
    s.relayers.clear();
    s
}

fn gen_env(g: &mut Gen) -> EnvMap {
    let mut env = EnvMap::default();
    for _ in 0..usize::arbitrary(g) % 5 {
        let prefix = if bool::arbitrary(g) { "CMT" } else { "FM" };
        let key = format!("{prefix}_{}", ResourceId::arbitrary(g).0);
        env.insert(key, String::arbitrary(g));
    }
    env
}

/// Choose some balance, up to 10% of the remaining balance of the account, minimum 1 atto.
///
/// Modify the reamaining balance so we don't run out.
fn gen_balance(g: &mut Gen, account_id: &AccountId, balances: &mut BalanceMap) -> Balance {
    let r = balances
        .get_mut(account_id)
        .expect("account doesn't have balance");
    let m = r.0.atto().div_ceil(&BigInt::from(10));
    let b = BigInt::arbitrary(g).mod_floor(&m).max(BigInt::from(1));
    let b = TokenAmount::from_atto(b);
    r.0.sub_assign(b.clone());
    Balance(b)
}

fn gen_collateral(g: &mut Gen, account_id: &AccountId, balances: &mut BalanceMap) -> Collateral {
    let b = gen_balance(g, account_id, balances);
    Collateral(b.0)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    use super::choose_at_least;

    #[derive(Clone, Debug)]
    struct TestSample {
        items: Vec<u8>,
        min_size: usize,
        sample: Vec<u8>,
    }

    impl Arbitrary for TestSample {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let mut items = HashSet::<u8>::arbitrary(g);
            items.insert(u8::arbitrary(g));
            let items = items.into_iter().collect::<Vec<_>>();
            let min_size = 1 + usize::arbitrary(g) % items.len();
            let sample = choose_at_least(g, min_size, &items);
            TestSample {
                items,
                min_size,
                sample,
            }
        }
    }

    #[quickcheck]
    fn test_sample_at_least(data: TestSample) {
        let sample_set = BTreeSet::from_iter(&data.sample);
        let item_set = BTreeSet::from_iter(&data.items);

        assert!(
            data.sample.len() >= data.min_size,
            "sampled at least the required amount"
        );
        assert!(
            data.sample.len() <= data.items.len(),
            "didn't sample more than available"
        );
        assert!(
            sample_set.is_subset(&item_set),
            "sample items are taken from the existing ones"
        );
        assert_eq!(data.sample.len(), sample_set.len(), "sample is unique");
    }
}
