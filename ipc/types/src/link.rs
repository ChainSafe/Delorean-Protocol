// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::actor_error;
use std::any::type_name;
use std::marker::PhantomData;

use super::{CodeType, TCid, TCidContent};
use crate::tcid_ops;
use anyhow::Result;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use std::ops::{Deref, DerefMut};

/// Static typing information for `Cid` fields to help read and write data safely.
///
/// # Example
/// ```
/// use ipc_types::{TCid, TLink};
/// use fvm_ipld_blockstore::MemoryBlockstore;
/// use fvm_ipld_encoding::tuple::*;
/// use fvm_ipld_encoding::Cbor;
///
/// #[derive(Serialize_tuple, Deserialize_tuple)]
/// struct MyType {
///   my_field: u64
/// }
/// impl Cbor for MyType {}
///
/// let store = MemoryBlockstore::new();
///
/// let mut my_ref: TCid<TLink<MyType>> = TCid::new_link(&store, &MyType { my_field: 0 }).unwrap();
///
/// my_ref.update(&store, |x| {
///   x.my_field += 1;
///   Ok(())
/// }).unwrap();
///
/// assert_eq!(1, my_ref.load(&store).unwrap().my_field);
/// ```
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TLink<T> {
    _phantom_t: PhantomData<T>,
}

impl<T> TCidContent for TLink<T> {}

pub struct StoreContent<'s, S: Blockstore, T> {
    store: &'s S,
    content: T,
}

impl<'s, S: 's + Blockstore, T> Deref for StoreContent<'s, S, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

impl<'s, S: 's + Blockstore, T> DerefMut for StoreContent<'s, S, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.content
    }
}

/// Operations on primitive types that can directly be read/written from/to CBOR.
impl<T, C: CodeType> TCid<TLink<T>, C>
where
    T: Serialize + DeserializeOwned,
{
    /// Initialize a `TCid` by storing a value as CBOR in the store and capturing the `Cid`.
    pub fn new_link<S: Blockstore>(store: &S, value: &T) -> Result<Self> {
        let cid = store.put_cbor(value, C::code())?;
        Ok(Self::from(cid))
    }

    /// Read the underlying `Cid` from the store, if it exists.
    pub fn maybe_load<'s, S: Blockstore>(
        &self,
        store: &'s S,
    ) -> Result<Option<StoreContent<'s, S, T>>> {
        Ok(store
            .get_cbor(&self.cid)?
            .map(|content| StoreContent { store, content }))
    }

    /// Put the value into the store and overwrite the `Cid`.
    pub fn flush<'s, S: Blockstore>(
        &mut self,
        value: StoreContent<'s, S, T>,
    ) -> Result<StoreContent<'s, S, T>> {
        let cid = value.store.put_cbor(&value.content, C::code())?;
        self.cid = cid;
        Ok(value)
    }
}

tcid_ops!(TLink<T : Serialize + DeserializeOwned>, C: CodeType => StoreContent<'s, S, T>);

/// This `Default` implementation is there in case we need to derive `Default` for something
/// that contains a `TCid`, but also to be used as a null pointer, in cases where using an
/// `Option<TCid<TLink<T>>>` is not the right choice.
///
/// For example if something has a previous link to a parent item in all cases bug Genesis,
/// it could be more convenient to use non-optional values, than to match cases all the time.
///
/// The reason we are not using `T::default()` to generate the CID is because it is highly
/// unlikely that the actual store we deal with won't have an entry for that, so we would
/// not be able to retrieve it anyway, and also because in Go we would have just a raw
/// `Cid` field, which, when empty, serializes as `"nil"`. We want to be compatible with that.
///
/// Also, using `T::default()` with non-optional fields would lead to infinite recursion.
impl<T, C: CodeType> Default for TCid<TLink<T>, C> {
    fn default() -> Self {
        Self::from(cid::Cid::default())
    }
}
