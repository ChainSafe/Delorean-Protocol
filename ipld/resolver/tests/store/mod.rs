// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use fvm_ipld_blockstore::Blockstore;
use ipc_ipld_resolver::missing_blocks::missing_blocks;
use libipld::Cid;
use libp2p_bitswap::BitswapStore;

#[derive(Debug, Clone, Default)]
pub struct TestBlockstore {
    blocks: Arc<RwLock<HashMap<Cid, Vec<u8>>>>,
}

impl Blockstore for TestBlockstore {
    fn has(&self, k: &Cid) -> Result<bool> {
        Ok(self.blocks.read().unwrap().contains_key(k))
    }

    fn get(&self, k: &Cid) -> Result<Option<Vec<u8>>> {
        Ok(self.blocks.read().unwrap().get(k).cloned())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> Result<()> {
        self.blocks.write().unwrap().insert(*k, block.into());
        Ok(())
    }
}

pub type TestStoreParams = libipld::DefaultParams;

impl BitswapStore for TestBlockstore {
    type Params = TestStoreParams;

    fn contains(&mut self, cid: &Cid) -> Result<bool> {
        Blockstore::has(self, cid)
    }

    fn get(&mut self, cid: &Cid) -> Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }

    fn insert(&mut self, block: &libipld::Block<Self::Params>) -> Result<()> {
        Blockstore::put_keyed(self, block.cid(), block.data())
    }

    fn missing_blocks(&mut self, cid: &Cid) -> Result<Vec<Cid>> {
        missing_blocks::<Self, Self::Params>(self, cid)
    }
}
