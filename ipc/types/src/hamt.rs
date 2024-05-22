// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::actor_error;
use std::any::type_name;
use std::marker::PhantomData;

use super::{make_empty_map, make_map_with_root_and_bitwidth};
use crate::tcid_ops;
use anyhow::{anyhow, Result};
use fvm_ipld_blockstore::{Blockstore, MemoryBlockstore};
use fvm_ipld_hamt::Error as HamtError;
use fvm_ipld_hamt::Hamt;
use fvm_shared::HAMT_BIT_WIDTH;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;

use super::{TCid, TCidContent};

/// Static typing information for HAMT fields, a.k.a. `Map`.
///
/// # Example
/// ```
/// use ipc_types::{TCid, THamt};
/// use fvm_ipld_blockstore::MemoryBlockstore;
/// use fvm_ipld_encoding::tuple::*;
/// use fvm_ipld_encoding::Cbor;
/// use fvm_ipld_hamt::BytesKey;
///
/// #[derive(Serialize_tuple, Deserialize_tuple)]
/// struct MyType {
///   my_field: TCid<THamt<String, u64>>
/// }
/// impl Cbor for MyType {}
///
/// let store = MemoryBlockstore::new();
///
/// let mut my_inst = MyType {
///   my_field: TCid::new_hamt(&store).unwrap()
/// };
///
/// let key = BytesKey::from("foo");
///
/// my_inst.my_field.update(&store, |x| {
///   x.set(key.clone(), 1).map_err(|e| e.into()).map(|_| ())
/// }).unwrap();
///
/// assert_eq!(&1, my_inst.my_field.load(&store).unwrap().get(&key).unwrap().unwrap())
/// ```
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct THamt<K, V, const W: u32 = HAMT_BIT_WIDTH> {
    _phantom_k: PhantomData<K>,
    _phantom_v: PhantomData<V>,
}

impl<K, V, const W: u32> TCidContent for THamt<K, V, W> {}

impl<K, V, const W: u32> TCid<THamt<K, V, W>>
where
    V: Serialize + DeserializeOwned,
{
    /// Initialize an empty data structure, flush it to the store and capture the `Cid`.
    pub fn new_hamt<S: Blockstore>(store: &S) -> Result<Self> {
        let cid = make_empty_map::<_, V>(store, W)
            .flush()
            .map_err(|e| anyhow!("Failed to create empty map: {:?}", e))?;

        Ok(Self::from(cid))
    }

    /// Load the data pointing at the store with the underlying `Cid` as its root, if it exists.
    pub fn maybe_load<'s, S: Blockstore>(&self, store: &'s S) -> Result<Option<Hamt<&'s S, V>>> {
        match make_map_with_root_and_bitwidth::<S, V>(&self.cid, store, W) {
            Ok(content) => Ok(Some(content)),
            Err(HamtError::CidNotFound(_)) => Ok(None),
            Err(other) => Err(anyhow!(other)),
        }
    }

    /// Flush the data to the store and overwrite the `Cid`.
    pub fn flush<'s, S: Blockstore>(
        &mut self,
        mut value: Hamt<&'s S, V>,
    ) -> Result<Hamt<&'s S, V>> {
        let cid = value
            .flush()
            .map_err(|e| anyhow!("error flushing {}: {:?}", type_name::<Self>(), e))?;
        self.cid = cid;
        Ok(value)
    }
}

tcid_ops!(THamt<K, V : Serialize + DeserializeOwned, W const: u32> => Hamt<&'s S, V>);

/// This `Default` implementation is unsound in that while it
/// creates `TCid` instances with a correct `Cid` value, this value
/// is not stored anywhere, so there is no guarantee that any retrieval
/// attempt from a random store won't fail.
///
/// The main purpose is to allow the `#[derive(Default)]` to be
/// applied on types that use a `TCid` field, if that's unavoidable.
impl<K, V, const W: u32> Default for TCid<THamt<K, V, W>>
where
    V: Serialize + DeserializeOwned,
{
    fn default() -> Self {
        Self::new_hamt(&MemoryBlockstore::new()).unwrap()
    }
}
