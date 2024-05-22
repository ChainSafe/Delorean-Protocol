use std::collections::{BTreeMap, BTreeSet};

// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::Context;
use cid::multihash::MultihashDigest;
use cid::Cid;
use fendermint_vm_genesis::{Actor, ActorMeta};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_hamt::Hamt;
use fvm_shared::{address::Address, ActorID, HAMT_BIT_WIDTH};

use crate::{eam::EthAddress, system};

/// Defines first available ID address after builtin actors
pub const FIRST_NON_SINGLETON_ADDR: ActorID = 100;

define_singleton!(INIT { id: 1, code_id: 2 });

pub type AddressMap = BTreeMap<Address, ActorID>;

/// Delegated address of an Ethereum built-in actor.
///
/// This is based on what seems to be going on in the `CREATE_EXTERNAL` method
/// of the EAM actor when it determines a robust address for an account ID,
/// in that we take something known (a public key, or in this case the ID),
/// hash it and truncate the results to 20 bytes.
///
/// But it's not a general rule of turning actor IDs into Ethereum addresses!
/// It's just something we do to assign an address that looks like an Ethereum one.
pub fn builtin_actor_eth_addr(id: ActorID) -> EthAddress {
    // The EVM actor would reject a delegated address that looks like an ID address, so let's hash it.
    // Based on `hash20` in the EAM actor:
    // https://github.com/filecoin-project/builtin-actors/blob/v11.0.0/actors/eam/src/lib.rs#L213-L216
    let eth_addr = EthAddress::from_id(id);
    let eth_addr = cid::multihash::Code::Keccak256.digest(&eth_addr.0);
    let eth_addr: [u8; 20] = eth_addr.digest()[12..32].try_into().unwrap();
    EthAddress(eth_addr)
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
pub struct State {
    pub address_map: Cid,
    pub next_id: ActorID,
    pub network_name: String,
    #[cfg(feature = "m2-native")]
    pub installed_actors: Cid,
}

// TODO: Not happy about having to copy this. Maybe we should use project references after all.
impl State {
    /// Create new state instance.
    pub fn new<BS: Blockstore>(
        store: &BS,
        network_name: String,
        // Accounts from the Genesis file.
        accounts: &[Actor],
        // Pre-defined IDs for top-level EVM contracts.
        eth_builtin_ids: &BTreeSet<ActorID>,
        // Number of dynamically deployed EVM library contracts.
        eth_library_count: u64,
    ) -> anyhow::Result<(Self, AddressMap)> {
        // Returning only the addreses that belong to user accounts.
        let mut allocated_ids = AddressMap::new();
        // Inserting both user accounts and built-in EVM actors.
        let mut address_map = Hamt::<&BS, ActorID>::new_with_bit_width(store, HAMT_BIT_WIDTH);

        let mut set_address = |addr: Address, id: ActorID| {
            tracing::debug!(
                addr = addr.to_string(),
                actor_id = id,
                "setting init address"
            );
            address_map.set(addr.to_bytes().into(), id)
        };

        let addresses = accounts.iter().flat_map(|a| match &a.meta {
            ActorMeta::Account(acc) => {
                vec![acc.owner.0]
            }
            ActorMeta::Multisig(ms) => ms.signers.iter().map(|a| a.0).collect(),
        });

        let mut next_id = FIRST_NON_SINGLETON_ADDR;

        for addr in addresses {
            if allocated_ids.contains_key(&addr) {
                continue;
            }
            allocated_ids.insert(addr, next_id);
            set_address(addr, next_id).context("cannot set ID of account address")?;
            next_id += 1;
        }

        // We will need to allocate an ID for each multisig account, however,
        // these do not have to be recorded in the map, because their addr->ID
        // mapping is trivial (it's an ID type address). To avoid the init actor
        // using the same ID for something else, give it a higher ID to use next.
        for a in accounts.iter() {
            if let ActorMeta::Multisig { .. } = a.meta {
                next_id += 1;
            }
        }

        // Insert top-level EVM contracts which have fixed IDs.
        for id in eth_builtin_ids {
            let addr = Address::from(builtin_actor_eth_addr(*id));
            set_address(addr, *id).context("cannot set ID of eth contract address")?;
        }

        // Insert dynamic EVM library contracts.
        for _ in 0..eth_library_count {
            let addr = Address::from(builtin_actor_eth_addr(next_id));
            set_address(addr, next_id).context("cannot set ID of eth library address")?;
            next_id += 1;
        }

        // Insert the null-Ethereum address to equal the system actor,
        // so the system actor can be identified by 0xff00..00 as well as 0x00..00
        set_address(*system::SYSTEM_ACTOR_ETH_ADDR, system::SYSTEM_ACTOR_ID)
            .context("cannot set ID of system eth address")?;

        #[cfg(feature = "m2-native")]
        let installed_actors = store.put_cbor(&Vec::<Cid>::new(), Code::Blake2b256)?;

        let state = Self {
            address_map: address_map.flush()?,
            next_id,
            network_name,
            #[cfg(feature = "m2-native")]
            installed_actors,
        };

        tracing::debug!(?state, "init actor state");

        Ok((state, allocated_ids))
    }
}
