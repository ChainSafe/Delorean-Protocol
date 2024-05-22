// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use fendermint_storage::Decode;
use fendermint_storage::Encode;
use fendermint_storage::KVResult;
use fendermint_storage::KVTransaction;
use fendermint_storage::KVWritable;
use fendermint_storage::KVWrite;
use fendermint_storage::{KVError, KVRead, KVReadable, KVStore};
use rocksdb::BoundColumnFamily;
use rocksdb::ErrorKind;
use rocksdb::OptimisticTransactionDB;
use rocksdb::SnapshotWithThreadMode;
use rocksdb::Transaction;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::thread;

use crate::RocksDb;

/// Cache column families to avoid further cloning on each access.
struct ColumnFamilyCache<'a> {
    db: &'a OptimisticTransactionDB,
    cfs: RefCell<BTreeMap<String, Arc<BoundColumnFamily<'a>>>>,
}

impl<'a> ColumnFamilyCache<'a> {
    fn new(db: &'a OptimisticTransactionDB) -> Self {
        Self {
            db,
            cfs: Default::default(),
        }
    }

    /// Look up a column family and pass it to a closure.
    /// Return an error if it doesn't exist.
    fn with_cf_handle<F, T>(&self, name: &str, f: F) -> KVResult<T>
    where
        F: FnOnce(&Arc<BoundColumnFamily<'a>>) -> KVResult<T>,
    {
        let mut cfs = self.cfs.borrow_mut();
        let cf = match cfs.get(name) {
            Some(cf) => cf,
            None => match self.db.cf_handle(name) {
                None => {
                    return Err(KVError::Unexpected(
                        anyhow!("column family {name} doesn't exist").into(),
                    ))
                }
                Some(cf) => {
                    cfs.insert(name.to_owned(), cf);
                    cfs.get(name).unwrap()
                }
            },
        };
        f(cf)
    }
}

/// For reads, we can just take a snapshot of the DB.
pub struct RocksDbReadTx<'a> {
    cache: ColumnFamilyCache<'a>,
    snapshot: SnapshotWithThreadMode<'a, OptimisticTransactionDB>,
}

/// For writes, we use a transaction which we'll either commit or roll back at the end.
pub struct RocksDbWriteTx<'a> {
    cache: ColumnFamilyCache<'a>,
    tx: ManuallyDrop<Transaction<'a, OptimisticTransactionDB>>,
}

impl<'a> RocksDbWriteTx<'a> {
    // This method takes the transaction without running the panicky destructor.
    fn take_tx(self) -> Transaction<'a, OptimisticTransactionDB> {
        let mut this = ManuallyDrop::new(self);
        unsafe { ManuallyDrop::take(&mut this.tx) }
    }
}

impl<S> KVReadable<S> for RocksDb
where
    S: KVStore<Repr = Vec<u8>>,
    S::Namespace: AsRef<str>,
{
    type Tx<'a> = RocksDbReadTx<'a>
    where
        Self: 'a;

    fn read(&self) -> Self::Tx<'_> {
        let snapshot = self.db.snapshot();
        RocksDbReadTx {
            cache: ColumnFamilyCache::new(&self.db),
            snapshot,
        }
    }
}

impl<S> KVWritable<S> for RocksDb
where
    S: KVStore<Repr = Vec<u8>>,
    S::Namespace: AsRef<str>,
{
    type Tx<'a> = RocksDbWriteTx<'a>
    where
        Self: 'a;

    fn write(&self) -> Self::Tx<'_> {
        RocksDbWriteTx {
            cache: ColumnFamilyCache::new(&self.db),
            tx: ManuallyDrop::new(self.db.transaction()),
        }
    }
}

impl<'a, S> KVRead<S> for RocksDbReadTx<'a>
where
    S: KVStore<Repr = Vec<u8>>,
    S::Namespace: AsRef<str>,
{
    fn get<K, V>(&self, ns: &S::Namespace, k: &K) -> KVResult<Option<V>>
    where
        S: Encode<K> + Decode<V>,
    {
        self.cache.with_cf_handle(ns.as_ref(), |cf| {
            let key = S::to_repr(k)?;

            let res = self
                .snapshot
                .get_cf(cf, key.as_ref())
                .map_err(to_kv_error)?;

            match res {
                Some(bz) => Ok(Some(S::from_repr(&bz)?)),
                None => Ok(None),
            }
        })
    }

    fn iterate<K, V>(&self, ns: &S::Namespace) -> impl Iterator<Item = KVResult<(K, V)>>
    where
        S: Decode<K> + Decode<V>,
        <S as KVStore>::Repr: Ord + 'static,
    {
        self.cache
            .with_cf_handle(ns.as_ref(), |cf| {
                let it = self.snapshot.iterator_cf(cf, rocksdb::IteratorMode::Start);

                let it = it.map(|res| res.map_err(to_kv_error)).map(|res| {
                    res.and_then(|(k, v)| {
                        let k: K = S::from_repr(&k.to_vec())?;
                        let v: V = S::from_repr(&v.to_vec())?;
                        Ok((k, v))
                    })
                });

                Ok(it)
            })
            .expect("just wrapped into ok")
    }
}

impl<'a, S> KVRead<S> for RocksDbWriteTx<'a>
where
    S: KVStore<Repr = Vec<u8>>,
    S::Namespace: AsRef<str>,
{
    fn get<K, V>(&self, ns: &S::Namespace, k: &K) -> KVResult<Option<V>>
    where
        S: Encode<K> + Decode<V>,
    {
        self.cache.with_cf_handle(ns.as_ref(), |cf| {
            let key = S::to_repr(k)?;

            let res = self.tx.get_cf(cf, key.as_ref()).map_err(to_kv_error)?;

            match res {
                Some(bz) => Ok(Some(S::from_repr(&bz)?)),
                None => Ok(None),
            }
        })
    }

    fn iterate<K, V>(&self, ns: &S::Namespace) -> impl Iterator<Item = KVResult<(K, V)>>
    where
        S: Decode<K> + Decode<V>,
        <S as KVStore>::Repr: Ord + 'static,
    {
        self.cache
            .with_cf_handle(ns.as_ref(), |cf| {
                let it = self.tx.iterator_cf(cf, rocksdb::IteratorMode::Start);

                let it = it.map(|res| res.map_err(to_kv_error)).map(|res| {
                    res.and_then(|(k, v)| {
                        let k: K = S::from_repr(&k.to_vec())?;
                        let v: V = S::from_repr(&v.to_vec())?;
                        Ok((k, v))
                    })
                });

                Ok(it)
            })
            .expect("just wrapped into ok")
    }
}

impl<'a, S> KVWrite<S> for RocksDbWriteTx<'a>
where
    S: KVStore<Repr = Vec<u8>>,
    S::Namespace: AsRef<str>,
{
    fn put<K, V>(&mut self, ns: &S::Namespace, k: &K, v: &V) -> KVResult<()>
    where
        S: Encode<K> + Encode<V>,
    {
        self.cache.with_cf_handle(ns.as_ref(), |cf| {
            let k = S::to_repr(k)?;
            let v = S::to_repr(v)?;

            self.tx
                .put_cf(cf, k.as_ref(), v.as_ref())
                .map_err(to_kv_error)?;

            Ok(())
        })
    }

    fn delete<K>(&mut self, ns: &S::Namespace, k: &K) -> KVResult<()>
    where
        S: Encode<K>,
    {
        self.cache.with_cf_handle(ns.as_ref(), |cf| {
            let k = S::to_repr(k)?;

            self.tx.delete_cf(cf, k.as_ref()).map_err(to_kv_error)?;

            Ok(())
        })
    }
}

impl<'a> KVTransaction for RocksDbWriteTx<'a> {
    fn commit(self) -> KVResult<()> {
        let tx = self.take_tx();
        tx.commit().map_err(to_kv_error)
    }

    fn rollback(self) -> KVResult<()> {
        let tx = self.take_tx();
        tx.rollback().map_err(to_kv_error)
    }
}

impl<'a> Drop for RocksDbWriteTx<'a> {
    fn drop(&mut self) {
        if !thread::panicking() {
            panic!("Transaction prematurely dropped. Must call `.commit()` or `.rollback()`.");
        }
    }
}

fn to_kv_error(e: rocksdb::Error) -> KVError {
    if e.kind() == ErrorKind::Busy {
        KVError::Conflict
    } else {
        KVError::Unexpected(Box::new(e))
    }
}

#[cfg(all(feature = "kvstore", test))]
mod tests {
    use std::borrow::Cow;

    use quickcheck::{QuickCheck, Testable};
    use serde::{de::DeserializeOwned, Serialize};

    use fendermint_storage::{testing::*, Codec, Decode, Encode, KVError, KVResult, KVStore};

    use crate::{RocksDb, RocksDbConfig};

    const TEST_COUNT: u64 = 20;

    #[derive(Clone)]
    struct TestKVStore;

    impl KVStore for TestKVStore {
        type Namespace = TestNamespace;
        type Repr = Vec<u8>;
    }

    impl<T: Serialize> Encode<T> for TestKVStore {
        fn to_repr(value: &T) -> KVResult<Cow<Self::Repr>> {
            fvm_ipld_encoding::to_vec(value)
                .map_err(|e| KVError::Codec(Box::new(e)))
                .map(Cow::Owned)
        }
    }
    impl<T: DeserializeOwned> Decode<T> for TestKVStore {
        fn from_repr(repr: &Self::Repr) -> KVResult<T> {
            fvm_ipld_encoding::from_slice(repr).map_err(|e| KVError::Codec(Box::new(e)))
        }
    }

    impl<T> Codec<T> for TestKVStore where TestKVStore: Encode<T> + Decode<T> {}

    fn new_backend() -> RocksDb {
        let dir = tempfile::Builder::new()
            .tempdir()
            .expect("error creating temporary path for db");
        let path = dir.path().join("rocksdb");
        let db = RocksDb::open(path, &RocksDbConfig::default()).expect("error creating RocksDB");

        // Create the column families the test will use.
        for name in test_namespaces() {
            let _ = db.new_cf_handle(name).unwrap();
        }

        db
    }

    // Not using the `#[quickcheck]` macro so I can run fewer tests becasue they are slow.
    fn run_quickcheck<F: Testable>(f: F) {
        QuickCheck::new().tests(TEST_COUNT).quickcheck(f)
    }

    #[test]
    fn writable() {
        run_quickcheck(
            (|data| {
                let backend = new_backend();
                check_writable::<TestKVStore>(&backend, data)
            }) as fn(TestData) -> bool,
        )
    }

    #[test]
    fn write_isolation() {
        run_quickcheck(
            (|data| {
                let backend = new_backend();
                check_write_isolation::<TestKVStore>(&backend, data)
            }) as fn(TestDataMulti<2>) -> bool,
        )
    }

    #[test]
    fn write_isolation_concurrent() {
        run_quickcheck(
            (|data1, data2| {
                let backend = new_backend();
                check_write_isolation_concurrent::<TestKVStore, _>(&backend, data1, data2)
            }) as fn(TestData, TestData) -> bool,
        )
    }

    #[test]
    fn write_serialization_concurrent() {
        run_quickcheck(
            (|data1, data2| {
                let backend = new_backend();
                check_write_serialization_concurrent::<TestKVStore, _>(&backend, data1, data2)
            }) as fn(TestData, TestData) -> bool,
        )
    }

    #[test]
    fn read_isolation() {
        run_quickcheck(
            (|data| {
                let backend = new_backend();
                check_read_isolation::<TestKVStore, _>(&backend, data)
            }) as fn(TestData) -> bool,
        )
    }
}
