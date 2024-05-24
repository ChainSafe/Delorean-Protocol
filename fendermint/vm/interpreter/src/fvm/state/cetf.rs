// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::fvm::store::ReadOnlyBlockstore;
use anyhow::anyhow;
use cid::Cid;
use fendermint_actor_cetf::{BlockHeight, Tag};
use fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ID;
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

/// Reads the CETF system actor state to retrieve the tag
pub fn get_tag_at_height<DB: Blockstore + Clone + 'static>(
    db: DB,
    state_root: &Cid,
    height: BlockHeight,
) -> anyhow::Result<Option<Tag>> {
    let bstore = ReadOnlyBlockstore::new(db);
    let state_tree = StateTree::new_from_root(&bstore, state_root)?;

    // get the actor state cid
    let actor_state_cid = match state_tree.get_actor(CETFSYSCALL_ACTOR_ID) {
        Ok(Some(actor_state)) => actor_state.state,
        Ok(None) => {
            return Err(anyhow!(
                "CETF actor id ({}) not found in state",
                CETFSYSCALL_ACTOR_ID
            ));
        }
        Err(err) => {
            return Err(anyhow!(
                "failed to get CETF actor ({}) state, error: {}",
                CETFSYSCALL_ACTOR_ID,
                err
            ));
        }
    };

    // get the actor state from the blockstore
    let actor_state: fendermint_actor_cetf::State =
        match state_tree.store().get_cbor(&actor_state_cid) {
            Ok(Some(v)) => v,
            Ok(None) => {
                return Err(anyhow!(
                    "CETF actor ({}) state not found",
                    CETFSYSCALL_ACTOR_ID
                ));
            }
            Err(err) => {
                return Err(anyhow!(
                    "failed to get CETF actor ({}) state, error: {}",
                    CETFSYSCALL_ACTOR_ID,
                    err
                ));
            }
        };

    Ok(actor_state.get_tag_at_height(&bstore, height)?)
}
