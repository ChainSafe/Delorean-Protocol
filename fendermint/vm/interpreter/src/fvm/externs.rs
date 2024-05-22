// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use fendermint_vm_actor_interface::chainmetadata::CHAINMETADATA_ACTOR_ID;
use fvm::{
    externs::{Chain, Consensus, Externs, Rand},
    state_tree::StateTree,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, DAG_CBOR};
use fvm_shared::clock::ChainEpoch;

use super::store::ReadOnlyBlockstore;

pub struct FendermintExterns<DB>
where
    DB: Blockstore + 'static,
{
    blockstore: DB,
    state_root: Cid,
}

impl<DB> FendermintExterns<DB>
where
    DB: Blockstore + 'static,
{
    pub fn new(blockstore: DB, state_root: Cid) -> Self {
        Self {
            blockstore,
            state_root,
        }
    }
}

impl<DB> Rand for FendermintExterns<DB>
where
    DB: Blockstore + 'static,
{
    fn get_chain_randomness(&self, _round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        Err(anyhow!("randomness not implemented"))
    }

    fn get_beacon_randomness(&self, _round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        Err(anyhow!("beacon not implemented"))
    }
}

impl<DB> Consensus for FendermintExterns<DB>
where
    DB: Blockstore + 'static,
{
    fn verify_consensus_fault(
        &self,
        _h1: &[u8],
        _h2: &[u8],
        _extra: &[u8],
    ) -> anyhow::Result<(Option<fvm_shared::consensus::ConsensusFault>, i64)> {
        unimplemented!("not expecting to use consensus faults")
    }
}

impl<DB> Chain for FendermintExterns<DB>
where
    DB: Blockstore + Clone + 'static,
{
    // for retreiving the tipset_cid, we load the chain metadata actor state
    // at the given state_root and retrieve the blockhash for the given epoch
    fn get_tipset_cid(&self, epoch: ChainEpoch) -> anyhow::Result<Cid> {
        // create a read only state tree from the state root
        let bstore = ReadOnlyBlockstore::new(&self.blockstore);
        let state_tree = StateTree::new_from_root(&bstore, &self.state_root)?;

        // get the chain metadata actor state cid
        let actor_state_cid = match state_tree.get_actor(CHAINMETADATA_ACTOR_ID) {
            Ok(Some(actor_state)) => actor_state.state,
            Ok(None) => {
                return Err(anyhow!(
                    "chain metadata actor id ({}) not found in state",
                    CHAINMETADATA_ACTOR_ID
                ));
            }
            Err(err) => {
                return Err(anyhow!(
                    "failed to get chain metadata actor ({}) state, error: {}",
                    CHAINMETADATA_ACTOR_ID,
                    err
                ));
            }
        };

        // get the chain metadata actor state from the blockstore
        let actor_state: fendermint_actor_chainmetadata::State =
            match state_tree.store().get_cbor(&actor_state_cid) {
                Ok(Some(v)) => v,
                Ok(None) => {
                    return Err(anyhow!(
                        "chain metadata actor ({}) state not found",
                        CHAINMETADATA_ACTOR_ID
                    ));
                }
                Err(err) => {
                    return Err(anyhow!(
                        "failed to get chain metadata actor ({}) state, error: {}",
                        CHAINMETADATA_ACTOR_ID,
                        err
                    ));
                }
            };

        match actor_state.get_block_hash(&bstore, epoch) {
            // the block hash retrieved from state was saved raw from how we received it
            // from Tendermint (which is Sha2_256) and we simply wrap it here in a cid
            Ok(Some(v)) => match Code::Blake2b256.wrap(&v) {
                Ok(w) => Ok(Cid::new_v1(DAG_CBOR, w)),
                Err(err) => Err(anyhow!("failed to wrap block hash, error: {}", err)),
            },
            Ok(None) => Ok(Cid::default()),
            Err(err) => Err(err),
        }
    }
}

impl<DB> Externs for FendermintExterns<DB> where DB: Blockstore + Clone + 'static {}
