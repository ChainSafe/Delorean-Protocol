// Copyright 2022-2024 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rocksdb::{
    ColumnFamilyDescriptor, ErrorKind, OptimisticTransactionDB, Options, WriteBatchWithTransaction,
};
use std::{path::Path, sync::Arc};

mod config;
mod error;

pub use config::RocksDbConfig;
pub use error::Error;

#[derive(Clone)]
pub struct RocksDb {
    pub db: Arc<OptimisticTransactionDB>,
    options: Options,
}

/// `RocksDb` is used as the KV store. Unlike the implementation in Forest
/// which is using the `DB` type, this one is using `OptimisticTransactionDB`
/// so that we can make use of transactions that can be rolled back.
///
/// Usage:
/// ```no_run
/// use fendermint_rocksdb::{RocksDb, RocksDbConfig};
///
/// let mut db = RocksDb::open("test_db", &RocksDbConfig::default()).unwrap();
/// ```
impl RocksDb {
    /// Open existing column families.
    pub fn open<P>(path: P, config: &RocksDbConfig) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let cfs: Vec<String> = Vec::new();
        Self::open_cf(path, config, cfs.iter())
    }

    /// Open existing column families and potentially create new ones, using the same config.
    pub fn open_cf<P, I, N>(path: P, config: &RocksDbConfig, cfs: I) -> Result<Self, Error>
    where
        P: AsRef<Path>,
        I: Iterator<Item = N>,
        N: AsRef<str>,
    {
        let db_opts: rocksdb::Options = config.into();
        let ex_cfs = Self::list_cf(&path, config)?;
        let ex_cfs = ex_cfs
            .into_iter()
            .map(|cf| ColumnFamilyDescriptor::new(cf, db_opts.clone()));

        let db = OptimisticTransactionDB::open_cf_descriptors(&db_opts, path, ex_cfs)?;

        let db = Self {
            db: Arc::new(db),
            options: db_opts,
        };

        for cf in cfs {
            if !db.has_cf_handle(cf.as_ref()) {
                db.new_cf_handle(cf.as_ref())?;
            }
        }

        Ok(db)
    }

    /// List existing column families in a database.
    ///
    /// These need to be passed to `open_cf` when we are reopening the database.
    /// If the database doesn't exist, the method returns an empty list.
    fn list_cf<P>(path: P, config: &RocksDbConfig) -> Result<Vec<String>, Error>
    where
        P: AsRef<Path>,
    {
        let db_opts: rocksdb::Options = config.into();
        match OptimisticTransactionDB::<rocksdb::MultiThreaded>::list_cf(&db_opts, path) {
            Ok(cfs) => Ok(cfs),
            Err(e) if e.kind() == ErrorKind::IOError => Ok(Vec::new()),
            Err(e) => Err(Error::Database(e)),
        }
    }

    pub fn get_statistics(&self) -> Option<String> {
        self.options.get_statistics()
    }

    pub fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(key).map_err(Error::from)
    }

    pub fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Ok(self.db.put(key, value)?)
    }

    pub fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.delete(key)?)
    }

    pub fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get_pinned(key)
            .map(|v| v.is_some())
            .map_err(Error::from)
    }

    pub fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut batch = WriteBatchWithTransaction::<true>::default();
        for (k, v) in values {
            batch.put(k, v);
        }
        Ok(self.db.write_without_wal(batch)?)
    }

    pub fn flush(&self) -> Result<(), Error> {
        self.db.flush().map_err(|e| Error::Other(e.to_string()))
    }

    /// Check if a column family exists
    pub fn has_cf_handle(&self, name: &str) -> bool {
        self.db.cf_handle(name).is_some()
    }

    /// Create a new column family, using the default options.
    ///
    /// Returns error if it already exists.
    pub fn new_cf_handle<'a>(&self, name: &'a str) -> Result<&'a str, Error> {
        if self.has_cf_handle(name) {
            return Err(Error::Other(format!(
                "column family '{name}' already exists"
            )));
        }
        self.db.create_cf(name, &self.options)?;
        Ok(name)
    }
}
