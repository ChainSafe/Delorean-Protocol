// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{
    hash::Hash,
    marker::PhantomData,
    mem,
    sync::{Arc, Mutex, MutexGuard},
    thread,
};

use crate::{
    Decode, Encode, KVError, KVRead, KVReadable, KVResult, KVStore, KVTransaction, KVWritable,
    KVWrite,
};

/// Read-only mode.
pub struct Read;
/// Read-write mode.
pub struct Write;

/// Immutable data multimap.
type IDataMap<S> = im::HashMap<
    <S as KVStore>::Namespace,
    im::HashMap<<S as KVStore>::Repr, Arc<<S as KVStore>::Repr>>,
>;

/// Given some `KVStore` type, the `InMemoryBackend` can be used to
/// emulate the same interface, but keep all the data in memory.
/// This can facilitate unit tests, but can be used for transient
/// storage as well, although STM has more granular access, and
/// the performance of this thing is likely to be terrible.
///
/// By default it serializes write transactions, which is required
/// for its correctness, but it can be disabled for the sake of
/// testing, or if writes only happen from the same task and
/// never concurrently.
///
/// Alternatively we could change the transaction implementation
/// to track individual puts/deletes and apply them in batch
/// at commit time. In that case if puts are commutative then
/// we could do multiple writes at the same time.
#[derive(Clone)]
pub struct InMemoryBackend<S: KVStore> {
    data: Arc<Mutex<IDataMap<S>>>,
    write_token: Arc<Mutex<()>>,
    lock_writes: bool,
}

impl<S: KVStore> InMemoryBackend<S> {
    pub fn new(lock_writes: bool) -> Self {
        Self {
            data: Arc::new(Mutex::new(Default::default())),
            write_token: Arc::new(Mutex::new(())),
            lock_writes,
        }
    }
}

impl<S: KVStore> Default for InMemoryBackend<S> {
    fn default() -> Self {
        // Locking is the only safe way to use writes from multiple threads.
        Self::new(true)
    }
}

impl<S: KVStore> KVReadable<S> for InMemoryBackend<S>
where
    S::Repr: Hash + Eq,
{
    type Tx<'a> = Transaction<'a, S, Read> where Self: 'a;

    /// Take a fresh snapshot, to isolate the effects of any further writes
    /// to the datastore from this read transaction.
    fn read(&self) -> Transaction<S, Read> {
        Transaction {
            backend: self,
            data: self.data.lock().unwrap().clone(),
            token: None,
            _mode: Read,
        }
    }
}

impl<S: KVStore> KVWritable<S> for InMemoryBackend<S>
where
    S::Repr: Hash + Eq,
{
    type Tx<'a>
    = Transaction<'a, S, Write>
    where
        Self: 'a;

    /// Take a snapshot to accumulate writes until they are committed.
    /// Take a write-lock on the data if necessary, but beware it doesn't work well with STM.
    fn write(&self) -> Transaction<S, Write> {
        // Take this lock first, otherwise we might be blocking `data` from anyone being able to commit.
        let token = if self.lock_writes {
            Some(self.write_token.lock().unwrap())
        } else {
            None
        };
        Transaction {
            backend: self,
            data: self.data.lock().unwrap().clone(),
            token,
            _mode: Write,
        }
    }
}

/// A transaction that can be read-only with no write lock taken,
/// or read-write, releasing the lock when it goes out of scope.
pub struct Transaction<'a, S: KVStore, M> {
    backend: &'a InMemoryBackend<S>,
    data: IDataMap<S>,
    token: Option<MutexGuard<'a, ()>>,
    _mode: M,
}

impl<'a, S: KVStore> KVTransaction for Transaction<'a, S, Write> {
    // An exclusive lock has already been taken.
    fn commit(mut self) -> KVResult<()> {
        let mut guard = self.backend.data.lock().unwrap();
        *guard = mem::take(&mut self.data);
        mem::take(&mut self.token);
        Ok(())
    }

    fn rollback(mut self) -> KVResult<()> {
        mem::take(&mut self.token);
        Ok(())
    }
}

impl<'a, S: KVStore, M> Drop for Transaction<'a, S, M> {
    fn drop(&mut self) {
        if self.token.is_some() && !thread::panicking() {
            panic!("Transaction prematurely dropped. Must call `.commit()` or `.rollback()`.");
        }
    }
}

impl<'a, S: KVStore, M> KVRead<S> for Transaction<'a, S, M>
where
    S::Repr: Hash + Eq,
{
    fn get<K, V>(&self, ns: &S::Namespace, k: &K) -> KVResult<Option<V>>
    where
        S: Encode<K> + Decode<V>,
    {
        if let Some(m) = self.data.get(ns) {
            let kr = S::to_repr(k)?;
            let v = m.get(&kr).map(|v| S::from_repr(v));
            return v.transpose();
        }
        Ok(None)
    }

    fn iterate<K, V>(&self, ns: &S::Namespace) -> impl Iterator<Item = KVResult<(K, V)>>
    where
        S: Decode<K> + Decode<V>,
        <S as KVStore>::Repr: Ord + 'static,
        K: 'static,
        V: 'static,
    {
        if let Some(m) = self.data.get(ns) {
            let mut items = m.iter().map(|(k, v)| (k, v.as_ref())).collect::<Vec<_>>();
            items.sort_by(|a, b| a.0.cmp(b.0));

            KVIter::<S, K, V>::new(items)
        } else {
            KVIter::empty()
        }
    }
}

impl<'a, S: KVStore> KVWrite<S> for Transaction<'a, S, Write>
where
    S::Repr: Hash + Eq,
{
    fn put<K, V>(&mut self, ns: &S::Namespace, k: &K, v: &V) -> KVResult<()>
    where
        S: Encode<K> + Encode<V>,
    {
        let mut m = self.data.get(ns).cloned().unwrap_or_default();
        let kr = S::to_repr(k)?;
        let vr = S::to_repr(v)?;
        m.insert(kr.into_owned(), Arc::new(vr.into_owned()));
        self.data.insert(ns.clone(), m);
        Ok(())
    }

    fn delete<K>(&mut self, ns: &S::Namespace, k: &K) -> KVResult<()>
    where
        S: Encode<K>,
    {
        if let Some(mut m) = self.data.get(ns).cloned() {
            let kr = S::to_repr(k)?;
            m.remove(&kr);
            self.data.insert(ns.clone(), m);
        }
        Ok(())
    }
}

struct KVIter<'a, S: KVStore, K, V> {
    items: Vec<(&'a S::Repr, &'a S::Repr)>,
    next: usize,
    phantom_v: PhantomData<V>,
    phantom_k: PhantomData<K>,
}

impl<'a, S, K, V> KVIter<'a, S, K, V>
where
    S: KVStore,
{
    pub fn new(items: Vec<(&'a S::Repr, &'a S::Repr)>) -> Self {
        KVIter {
            items,
            next: 0,
            phantom_v: PhantomData,
            phantom_k: PhantomData,
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }
}

impl<'a, S, K, V> Iterator for KVIter<'a, S, K, V>
where
    S: KVStore + Decode<K> + Decode<V>,
{
    type Item = Result<(K, V), KVError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((k, v)) = self.items.get(self.next) {
            self.next += 1;
            let kv = S::from_repr(k).and_then(|k| S::from_repr(v).map(|v| (k, v)));
            Some(kv)
        } else {
            None
        }
    }
}

#[cfg(all(feature = "inmem", test))]
mod tests {
    use std::borrow::Cow;

    use crate::{im::InMemoryBackend, Codec, Decode, Encode, KVError, KVResult, KVStore};
    use quickcheck_macros::quickcheck;
    use serde::{de::DeserializeOwned, Serialize};

    use crate::testing::*;

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

    #[quickcheck]
    fn writable(data: TestData) -> bool {
        let backend = InMemoryBackend::<TestKVStore>::default();
        check_writable(&backend, data)
    }

    #[quickcheck]
    fn write_isolation(data: TestDataMulti<2>) -> bool {
        // XXX: It isn't safe to use this backend without locking writes if writes are concurrent.
        // It's just here to try the test on something.
        let backend = InMemoryBackend::<TestKVStore>::new(false);
        check_write_isolation(&backend, data)
    }

    #[quickcheck]
    fn write_isolation_concurrent(data1: TestData, data2: TestData) -> bool {
        let backend = InMemoryBackend::<TestKVStore>::default();
        check_write_isolation_concurrent(&backend, data1, data2)
    }

    #[quickcheck]
    fn write_serialization_concurrent(data1: TestData, data2: TestData) -> bool {
        let backend = InMemoryBackend::<TestKVStore>::default();
        check_write_serialization_concurrent(&backend, data1, data2)
    }

    #[quickcheck]
    fn read_isolation(data: TestData) -> bool {
        let backend = InMemoryBackend::<TestKVStore>::default();
        check_read_isolation(&backend, data)
    }
}
