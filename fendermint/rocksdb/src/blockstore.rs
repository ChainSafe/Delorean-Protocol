use std::sync::Arc;

// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use rocksdb::{BoundColumnFamily, OptimisticTransactionDB, WriteBatchWithTransaction};

use crate::RocksDb;

impl Blockstore for RocksDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.read(k.to_bytes())?)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        Ok(self.write(k.to_bytes(), block)?)
    }

    // Called by the BufferedBlockstore during flush.
    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        let mut batch = WriteBatchWithTransaction::<true>::default();
        for (cid, v) in blocks.into_iter() {
            let k = cid.to_bytes();
            let v = v.as_ref();
            batch.put(k, v);
        }
        // This function is used in `fvm_ipld_car::load_car`
        // It reduces time cost of loading mainnet snapshot
        // by ~10% by not writing to WAL(write ahead log).
        // Ok(self.db.write_without_wal(batch)?)

        // For some reason with the `write_without_wal` version if I restart the application
        // it doesn't find the manifest root.
        Ok(self.db.write(batch)?)
    }
}

/// A [`Blockstore`] implementation that writes to a specific namespace, not the default like above.
#[derive(Clone)]
pub struct NamespaceBlockstore {
    db: Arc<OptimisticTransactionDB>,
    ns: String,
}

impl NamespaceBlockstore {
    pub fn new(db: RocksDb, ns: String) -> anyhow::Result<Self> {
        // All namespaces are pre-created during open.
        if !db.has_cf_handle(&ns) {
            Err(anyhow!("namespace {ns} does not exist!"))
        } else {
            Ok(Self { db: db.db, ns })
        }
    }

    // Unfortunately there doesn't seem to be a way to avoid having to
    // clone another instance for each operation :(
    fn cf(&self) -> anyhow::Result<Arc<BoundColumnFamily>> {
        self.db
            .cf_handle(&self.ns)
            .ok_or_else(|| anyhow!("namespace {} does not exist!", self.ns))
    }
}

impl Blockstore for NamespaceBlockstore {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.db.get_cf(&self.cf()?, k.to_bytes())?)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        Ok(self.db.put_cf(&self.cf()?, k.to_bytes(), block)?)
    }

    // Called by the BufferedBlockstore during flush.
    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        let cf = self.cf()?;
        let mut batch = WriteBatchWithTransaction::<true>::default();
        for (cid, v) in blocks.into_iter() {
            let k = cid.to_bytes();
            let v = v.as_ref();
            batch.put_cf(&cf, k, v);
        }
        Ok(self.db.write(batch)?)
    }
}
