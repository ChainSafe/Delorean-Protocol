// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context};

use cid::Cid;
use fendermint_vm_core::chainid::HasChainID;
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::chainid::ChainID;

use crate::fvm::store::ReadOnlyBlockstore;

/// A state we create for the execution of all the messages in a block.
pub struct FvmCheckState<DB>
where
    DB: Blockstore + Clone + 'static,
{
    state_tree: StateTree<ReadOnlyBlockstore<DB>>,
    chain_id: ChainID,
}

impl<DB> FvmCheckState<DB>
where
    DB: Blockstore + Clone + 'static,
{
    pub fn new(blockstore: DB, state_root: Cid, chain_id: ChainID) -> anyhow::Result<Self> {
        // Sanity check that the blockstore contains the supplied state root.
        if !blockstore
            .has(&state_root)
            .context("failed to load initial state-root")?
        {
            return Err(anyhow!(
                "blockstore doesn't have the initial state-root {}",
                state_root
            ));
        }

        // Create a new state tree from the supplied root.
        let state_tree = {
            let bstore = ReadOnlyBlockstore::new(blockstore);
            StateTree::new_from_root(bstore, &state_root)?
        };

        let state = Self {
            state_tree,
            chain_id,
        };

        Ok(state)
    }

    pub fn state_tree_mut(&mut self) -> &mut StateTree<ReadOnlyBlockstore<DB>> {
        &mut self.state_tree
    }
}

impl<DB> HasChainID for FvmCheckState<DB>
where
    DB: Blockstore + Clone + 'static,
{
    fn chain_id(&self) -> ChainID {
        self.chain_id
    }
}
