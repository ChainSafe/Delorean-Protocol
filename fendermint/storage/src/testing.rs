// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::{
    Codec, KVCollection, KVError, KVRead, KVReadable, KVStore, KVTransaction, KVWritable, KVWrite,
};
use quickcheck::{Arbitrary, Gen};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::thread;

/// We'll see how this works out. We would have to wrap any KVStore
/// with something that can handle strings as namespaces.
pub type TestNamespace = &'static str;

/// Return all namespaces used by the tests, so they can be pre-allocated, if necessary.
pub fn test_namespaces() -> &'static [&'static str] {
    ["fizz", "buzz", "spam", "eggs"].as_slice()
}

/// Test operations on some collections with known types,
/// so we can have the simplest possible model implementation.
#[derive(Clone, Debug)]
pub enum TestOpKV<K, V> {
    Get(K),
    Put(K, V),
    Del(K),
    Iter,
}

#[derive(Clone, Debug)]
pub enum TestOpNs {
    /// String-to-Int
    S2I(TestNamespace, TestOpKV<String, u8>),
    /// Int-to-String
    I2S(TestNamespace, TestOpKV<u8, String>),
    Rollback,
}

#[derive(Clone, Debug)]
pub struct TestData {
    ops: Vec<TestOpNs>,
}

/// Generate commands from a limited set of keys so there's a
/// high probability that we get/delete what we put earlier.
impl Arbitrary for TestOpNs {
    fn arbitrary(g: &mut Gen) -> Self {
        use TestOpKV::*;
        use TestOpNs::*;
        match u8::arbitrary(g) % 100 {
            i if i < 47 => {
                let ns = g.choose(&["spam", "eggs"]).unwrap();
                let k = *g.choose(&["foo", "bar", "baz"]).unwrap();
                match u8::arbitrary(g) % 10 {
                    i if i < 3 => S2I(ns, Get(k.to_owned())),
                    i if i < 4 => S2I(ns, Iter),
                    i if i < 9 => S2I(ns, Put(k.to_owned(), Arbitrary::arbitrary(g))),
                    _ => S2I(ns, Del(k.to_owned())),
                }
            }
            i if i < 94 => {
                let ns = g.choose(&["fizz", "buzz"]).unwrap();
                let k = u8::arbitrary(g) % 3;
                match u8::arbitrary(g) % 10 {
                    i if i < 3 => I2S(ns, Get(k)),
                    i if i < 4 => I2S(ns, Iter),
                    i if i < 9 => {
                        let sz = u8::arbitrary(g) % 5;
                        let s = (0..sz).map(|_| char::arbitrary(g)).collect();
                        I2S(ns, Put(k, s))
                    }
                    _ => I2S(ns, Del(k)),
                }
            }
            _ => Rollback,
        }
    }
}

impl Arbitrary for TestData {
    fn arbitrary(g: &mut Gen) -> Self {
        TestData {
            ops: Arbitrary::arbitrary(g),
        }
    }
}

/// Test data for multiple transactions, interspersed.
#[derive(Clone, Debug)]
pub struct TestDataMulti<const N: usize> {
    ops: Vec<(usize, TestOpNs)>,
}

impl<const N: usize> Arbitrary for TestDataMulti<N> {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut ops = Vec::new();
        for i in 0..N {
            let data = TestData::arbitrary(g);
            ops.extend(data.ops.into_iter().map(|op| (i32::arbitrary(g), i, op)));
        }
        ops.sort_by_key(|(r, _, _)| *r);

        TestDataMulti {
            ops: ops.into_iter().map(|(_, i, op)| (i, op)).collect(),
        }
    }
}

pub struct TestDataStore;

impl KVStore for TestDataStore {
    type Namespace = TestNamespace;
    type Repr = Vec<u8>;
}

#[derive(Default)]
struct Model {
    s2i: HashMap<TestNamespace, HashMap<String, u8>>,
    i2s: HashMap<TestNamespace, HashMap<u8, String>>,
}

struct Collections<S: KVStore> {
    s2i: HashMap<TestNamespace, KVCollection<S, String, u8>>,
    i2s: HashMap<TestNamespace, KVCollection<S, u8, String>>,
}

impl<S: KVStore> Default for Collections<S> {
    fn default() -> Self {
        Self {
            s2i: HashMap::new(),
            i2s: HashMap::new(),
        }
    }
}

impl<S> Collections<S>
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<String> + Codec<u8>,
{
    fn s2i(&mut self, ns: TestNamespace) -> &KVCollection<S, String, u8> {
        self.s2i.entry(ns).or_insert_with(|| KVCollection::new(ns))
    }

    fn i2s(&mut self, ns: TestNamespace) -> &KVCollection<S, u8, String> {
        self.i2s.entry(ns).or_insert_with(|| KVCollection::new(ns))
    }
}

/// State machine test for an implementation of a `KVWritable` using a sequence of random ops.
pub fn check_writable<S>(sut: &impl KVWritable<S>, data: TestData) -> bool
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<String> + Codec<u8>,
    S::Repr: Ord + 'static,
{
    let mut model = Model::default();
    // Creating a collection doesn't add much to the test but at least we exercise this path.
    let mut colls = Collections::<S>::default();
    // Start the transaction.
    let mut tx = sut.write();
    let mut ok = true;
    for d in data.ops {
        match d {
            TestOpNs::S2I(ns, op) => {
                let coll = colls.s2i(ns);
                if !apply_both(&mut tx, &mut model.s2i, coll, ns, op) {
                    ok = false;
                }
            }
            TestOpNs::I2S(ns, op) => {
                let coll = colls.i2s(ns);
                if !apply_both(&mut tx, &mut model.i2s, coll, ns, op) {
                    ok = false;
                }
            }
            TestOpNs::Rollback => {
                //println!("ROLLBACK");
                model = Model::default();
                tx.rollback().unwrap();
                tx = sut.write();
            }
        }
    }
    tx.rollback().unwrap();
    ok
}

/// Check that two write transactions don't see each others' changes.
///
/// This test assumes that write transactions can be executed concurrently, that
/// they don't block each other. If that's not the case don't call this test.
pub fn check_write_isolation<S>(sut: &impl KVWritable<S>, data: TestDataMulti<2>) -> bool
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<String> + Codec<u8>,
    S::Repr: Ord + 'static,
{
    let mut colls = Collections::<S>::default();
    let mut model1 = Model::default();
    let mut model2 = Model::default();
    let mut tx1 = sut.write();
    let mut tx2 = sut.write();
    let mut ok = true;
    for (i, op) in data.ops {
        let tx = if i == 0 { &mut tx1 } else { &mut tx2 };
        let model = if i == 0 { &mut model1 } else { &mut model2 };
        match op {
            TestOpNs::S2I(ns, op) => {
                let coll = colls.s2i(ns);
                if !apply_both(tx, &mut model.s2i, coll, ns, op) {
                    ok = false;
                }
            }
            TestOpNs::I2S(ns, op) => {
                let coll = colls.i2s(ns);
                if !apply_both(tx, &mut model.i2s, coll, ns, op) {
                    ok = false;
                }
            }
            TestOpNs::Rollback => {}
        }
    }
    tx1.rollback().unwrap();
    tx2.rollback().unwrap();
    ok
}

/// Check that two write transactions don't see each others' changes when executed on different threads.
pub fn check_write_isolation_concurrent<S, B>(sut: &B, data1: TestData, data2: TestData) -> bool
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<String> + Codec<u8>,
    S::Repr: Ord + 'static,
    B: KVWritable<S> + Clone + Send + 'static,
{
    let sut2 = sut.clone();
    let t = thread::spawn(move || check_writable(&sut2, data2));
    let c1 = check_writable(sut, data1);
    let c2 = t.join().unwrap();
    c1 && c2
}

/// Check that two write transactions are serializable, their effects don't get lost and aren't interspersed.
pub fn check_write_serialization_concurrent<S, B>(sut: &B, data1: TestData, data2: TestData) -> bool
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<String> + Codec<u8>,
    B: KVWritable<S> + KVReadable<S> + Clone + Send + 'static,
{
    // Tests can now fail during writes because they realise some other transaction has already committed.
    let try_apply_sut = |sut: &B, data: &TestData| -> Result<(), KVError> {
        let mut tx = sut.write();
        for op in data.ops.iter() {
            match op {
                TestOpNs::S2I(ns, TestOpKV::Put(k, v)) => tx.put(ns, k, v)?,
                TestOpNs::S2I(ns, TestOpKV::Del(k)) => tx.delete(ns, k)?,
                TestOpNs::I2S(ns, TestOpKV::Put(k, v)) => tx.put(ns, k, v)?,
                TestOpNs::I2S(ns, TestOpKV::Del(k)) => tx.delete(ns, k)?,
                _ => (),
            }
        }
        tx.commit()
    };

    // Try to apply once, if it fails due to conflict, retry, otherwise panic.
    let apply_sut = move |sut: &B, data: &TestData| match try_apply_sut(sut, data) {
        Err(KVError::Conflict) => try_apply_sut(sut, data).unwrap(),
        Err(other) => panic!("error applying test data: {other:?}"),
        Ok(()) => (),
    };

    let sutc = sut.clone();
    let data2c = data2.clone();
    let t = thread::spawn(move || apply_sut(&sutc, &data2c));
    apply_sut(sut, &data1);
    t.join().unwrap();

    // The changes were applied in one order or the other.
    let tx = sut.read();
    let apply_model = |a: &TestData, b: &TestData| -> bool {
        let mut model = Model::default();
        // First apply all the writes
        for op in a.ops.iter().chain(b.ops.iter()).cloned() {
            match op {
                TestOpNs::S2I(ns, TestOpKV::Put(k, v)) => {
                    model.s2i.entry(ns).or_default().insert(k, v);
                }
                TestOpNs::S2I(ns, TestOpKV::Del(k)) => {
                    model.s2i.entry(ns).or_default().remove(&k);
                }
                TestOpNs::I2S(ns, TestOpKV::Put(k, v)) => {
                    model.i2s.entry(ns).or_default().insert(k, v);
                }
                TestOpNs::I2S(ns, TestOpKV::Del(k)) => {
                    model.i2s.entry(ns).or_default().remove(&k);
                }
                _ => (),
            }
        }
        // Then just the reads on the final state.
        for op in a.ops.iter().chain(b.ops.iter()) {
            match op {
                TestOpNs::S2I(ns, TestOpKV::Get(k)) => {
                    let expected = tx.get::<String, u8>(ns, k).unwrap();
                    let found = model.s2i.get(ns).and_then(|m| m.get(k)).cloned();
                    if found != expected {
                        return false;
                    }
                }
                TestOpNs::I2S(ns, TestOpKV::Get(k)) => {
                    let expected = tx.get::<u8, String>(ns, k).unwrap();
                    let found = model.i2s.get(ns).and_then(|m| m.get(k)).cloned();
                    if found != expected {
                        return false;
                    }
                }
                _ => (),
            }
        }
        true
    };

    let ok = apply_model(&data1, &data2) || apply_model(&data2, &data1);
    drop(tx);
    ok
}

/// Check that read transactions don't see changes from write transactions.
///
/// This test assumes that read and write transactions can be executed concurrently,
/// that they don't block each other. If that's not the case don't call this test.
pub fn check_read_isolation<S, B>(sut: &B, data: TestData) -> bool
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<String> + Codec<u8>,
    S::Repr: Ord + 'static,
    B: KVWritable<S> + KVReadable<S>,
{
    let mut model = Model::default();
    let mut colls = Collections::<S>::default();
    let mut txw = sut.write();
    let mut txr = sut.read();
    let mut gets = Vec::new();
    let mut ok = true;

    for op in data.ops {
        if let TestOpNs::S2I(ns, op) = op {
            let coll = colls.s2i(ns);
            apply_both(&mut txw, &mut model.s2i, coll, ns, op.clone());
            if let TestOpKV::Get(k) = &op {
                if coll.get(&txr, k).unwrap().is_some() {
                    ok = false;
                }
                gets.push((ns, op));
            }
        }
    }

    // Commit the writes, but they should still not be visible to the reads that started earlier.
    txw.commit().unwrap();

    for (ns, op) in &gets {
        let coll = colls.s2i(ns);
        if let TestOpKV::Get(k) = op {
            let found = coll.get(&txr, k).unwrap();
            if found.is_some() {
                ok = false;
            }
        }
    }

    // Finish the reads and start another read transaction. Now the writes should be visible.
    drop(txr);
    txr = sut.read();

    for (ns, op) in &gets {
        let coll = colls.s2i(ns);
        if let TestOpKV::Get(k) = op {
            let found = coll.get(&txr, k).unwrap();
            let expected = model.s2i.get(ns).and_then(|m| m.get(k)).cloned();
            if found != expected {
                ok = false;
            }
        }
    }

    ok
}

/// Apply an operation on the model and the KV store, checking that the results are the same where possible.
fn apply_both<S, K, V>(
    tx: &mut impl KVWrite<S>,
    model: &mut HashMap<TestNamespace, HashMap<K, V>>,
    coll: &KVCollection<S, K, V>,
    ns: TestNamespace,
    op: TestOpKV<K, V>,
) -> bool
where
    S: KVStore<Namespace = TestNamespace> + Clone + Codec<K> + Codec<V>,
    K: Clone + Hash + Eq + 'static,
    V: Clone + PartialEq + 'static,
    S::Repr: Ord + 'static,
{
    match op {
        TestOpKV::Get(k) => {
            let found = coll.get(tx, &k).unwrap();
            let expected = model.get(ns).and_then(|m| m.get(&k)).cloned();
            //println!("GET {:?}/{:?}: {:?} ?= {:?}", ns, k, found, expected);
            if found != expected {
                return false;
            }
        }
        TestOpKV::Put(k, v) => {
            //println!("PUT {:?}/{:?}: {:?}", ns, k, v);
            coll.put(tx, &k, &v).unwrap();
            model.entry(ns).or_default().insert(k, v);
        }
        TestOpKV::Del(k) => {
            //println!("DEL {:?}/{:?}", ns, k);
            coll.delete(tx, &k).unwrap();
            model.entry(ns).or_default().remove(&k);
        }
        TestOpKV::Iter => {
            let found = coll.iterate(tx).collect::<Result<Vec<_>, _>>().unwrap();

            let expected = if let Some(m) = model.get(ns) {
                let mut expected = m
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>();

                expected.sort_by(|a, b| {
                    let ka = S::to_repr(&a.0).unwrap();
                    let kb = S::to_repr(&b.0).unwrap();
                    ka.cmp(&kb)
                });

                expected
            } else {
                Vec::new()
            };

            if found != expected {
                return false;
            }
        }
    }
    true
}
