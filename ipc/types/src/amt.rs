// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::actor_error;
use std::any::type_name;
use std::marker::PhantomData;

use crate::tcid_ops;

use super::{TCid, TCidContent};
use anyhow::{anyhow, Result};
use fvm_ipld_amt::Amt;
use fvm_ipld_amt::Error as AmtError;
use fvm_ipld_blockstore::{Blockstore, MemoryBlockstore};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;

/// Same as `fvm_ipld_amt::DEFAULT_BIT_WIDTH`.
const AMT_BIT_WIDTH: u32 = 3;

/// Static typing information for AMT fields, a.k.a. `Array`.
///
/// # Example
/// ```
/// use ipc_types::{TCid, TAmt};
/// use fvm_ipld_blockstore::MemoryBlockstore;
/// use fvm_ipld_encoding::tuple::*;
/// use fvm_ipld_encoding::Cbor;
///
/// #[derive(Serialize_tuple, Deserialize_tuple)]
/// struct MyType {
///   my_field: TCid<TAmt<String>>
/// }
/// impl Cbor for MyType {}
///
/// let store = MemoryBlockstore::new();
///
/// let mut my_inst = MyType {
///   my_field: TCid::new_amt(&store).unwrap()
/// };
///
/// my_inst.my_field.update(&store, |x| {
///   x.set(0, "bar".into()).map_err(|e| e.into())
/// }).unwrap();
///
/// assert_eq!(&"bar", my_inst.my_field.load(&store).unwrap().get(0).unwrap().unwrap())
/// ```
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TAmt<V, const W: u32 = AMT_BIT_WIDTH> {
    _phantom_v: PhantomData<V>,
}

impl<V, const W: u32> TCidContent for TAmt<V, W> {}

impl<V, const W: u32> TCid<TAmt<V, W>>
where
    V: Serialize + DeserializeOwned,
{
    /// Initialize an empty data structure, flush it to the store and capture the `Cid`.
    pub fn new_amt<S: Blockstore>(store: &S) -> Result<Self> {
        let cid = Amt::<V, _>::new_with_bit_width(store, W)
            .flush()
            .map_err(|e| anyhow!("Failed to create empty array: {}", e))?;

        Ok(Self::from(cid))
    }

    /// Load the data pointing at the store with the underlying `Cid` as its root, if it exists.
    pub fn maybe_load<'s, S: Blockstore>(&self, store: &'s S) -> Result<Option<Amt<V, &'s S>>> {
        match Amt::<V, _>::load(&self.cid, store) {
            Ok(content) => Ok(Some(content)),
            Err(AmtError::CidNotFound(_)) => Ok(None),
            Err(other) => Err(anyhow!(other)),
        }
    }

    pub fn flush<'s, S: Blockstore>(&mut self, mut value: Amt<V, &'s S>) -> Result<Amt<V, &'s S>> {
        let cid = value
            .flush()
            .map_err(|e| anyhow!("error flushing {}: {}", type_name::<Self>(), e))?;
        self.cid = cid;
        Ok(value)
    }
}

tcid_ops!(TAmt<V : Serialize + DeserializeOwned, W const: u32> => Amt<V, &'s S>);

/// This `Default` implementation is unsound in that while it
/// creates `TAmt` instances with a correct `Cid` value, this value
/// is not stored anywhere, so there is no guarantee that any retrieval
/// attempt from a random store won't fail.
///
/// The main purpose is to allow the `#[derive(Default)]` to be
/// applied on types that use a `TAmt` field, if that's unavoidable.
impl<V, const W: u32> Default for TCid<TAmt<V, W>>
where
    V: Serialize + DeserializeOwned,
{
    fn default() -> Self {
        Self::new_amt(&MemoryBlockstore::new()).unwrap()
    }
}
