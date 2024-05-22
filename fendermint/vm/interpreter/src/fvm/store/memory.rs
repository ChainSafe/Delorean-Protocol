// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

/// An in-memory blockstore that can be shared between threads,
/// unlike [fvm_ipld_blockstore::memory::MemoryBlockstore].
#[derive(Debug, Default, Clone)]
pub struct MemoryBlockstore {
    blocks: Arc<RwLock<HashMap<Cid, Vec<u8>>>>,
}

impl MemoryBlockstore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Blockstore for MemoryBlockstore {
    fn has(&self, k: &Cid) -> Result<bool> {
        let guard = self.blocks.read().unwrap();
        Ok(guard.contains_key(k))
    }

    fn get(&self, k: &Cid) -> Result<Option<Vec<u8>>> {
        let guard = self.blocks.read().unwrap();
        Ok(guard.get(k).cloned())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> Result<()> {
        let mut guard = self.blocks.write().unwrap();
        guard.insert(*k, block.into());
        Ok(())
    }
}
