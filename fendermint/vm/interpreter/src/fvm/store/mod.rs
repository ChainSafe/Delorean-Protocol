// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::EMPTY_ARR_CID;

pub mod memory;

#[derive(Clone)]
pub struct ReadOnlyBlockstore<DB>(DB);

impl<DB> ReadOnlyBlockstore<DB> {
    pub fn new(store: DB) -> Self {
        Self(store)
    }
}

impl<DB> Blockstore for ReadOnlyBlockstore<DB>
where
    DB: Blockstore + Clone,
{
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.0.get(k)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        // The FVM inserts this each time to make sure it exists.
        if *k == EMPTY_ARR_CID {
            return self.0.put_keyed(k, block);
        }
        panic!("never intended to use put on the read-only blockstore")
    }
}
