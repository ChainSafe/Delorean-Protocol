use cid::Cid;
use ipc_ipld_resolver::missing_blocks::missing_blocks;
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use libp2p_bitswap::BitswapStore;
use std::borrow::Cow;

use fendermint_rocksdb::blockstore::NamespaceBlockstore;
use fendermint_storage::{Codec, Decode, Encode, KVError, KVResult, KVStore};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{de::DeserializeOwned, serde::Serialize};

/// [`KVStore`] type we use to store historial data in the database.
#[derive(Clone)]
pub struct AppStore;

impl KVStore for AppStore {
    type Repr = Vec<u8>;
    type Namespace = String;
}

impl<T> Codec<T> for AppStore where AppStore: Encode<T> + Decode<T> {}

/// CBOR serialization.
impl<T> Encode<T> for AppStore
where
    T: Serialize,
{
    fn to_repr(value: &T) -> KVResult<Cow<Self::Repr>> {
        fvm_ipld_encoding::to_vec(value)
            .map_err(|e| KVError::Codec(Box::new(e)))
            .map(Cow::Owned)
    }
}

/// CBOR deserialization.
impl<T> Decode<T> for AppStore
where
    T: DeserializeOwned,
{
    fn from_repr(repr: &Self::Repr) -> KVResult<T> {
        fvm_ipld_encoding::from_slice(repr).map_err(|e| KVError::Codec(Box::new(e)))
    }
}

/// A `Blockstore` and `BitswapStore` implementation we can pass to the IPLD Resolver.
pub struct BitswapBlockstore {
    /// The `Blockstore` implementation where we the FVM actors store their data.
    ///
    /// This must not be written to by Bitswap operations, because that could result
    /// in some nodes having some data that others don't, which would lead to a
    /// consensu failure. We can use read data from it, but not write to it.
    state_store: NamespaceBlockstore,
    /// The `Blockstore` implementation where Bitswap operations can write to.
    bit_store: NamespaceBlockstore,
}

impl BitswapBlockstore {
    pub fn new(state_store: NamespaceBlockstore, bit_store: NamespaceBlockstore) -> Self {
        Self {
            state_store,
            bit_store,
        }
    }
}

impl Blockstore for BitswapBlockstore {
    fn has(&self, k: &cid::Cid) -> anyhow::Result<bool> {
        if self.bit_store.has(k)? {
            Ok(true)
        } else {
            self.state_store.has(k)
        }
    }

    fn get(&self, k: &cid::Cid) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(data) = self.bit_store.get(k)? {
            Ok(Some(data))
        } else {
            self.state_store.get(k)
        }
    }

    fn put_keyed(&self, k: &cid::Cid, block: &[u8]) -> anyhow::Result<()> {
        self.bit_store.put_keyed(k, block)
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (cid::Cid, D)>,
    {
        self.bit_store.put_many_keyed(blocks)
    }
}

impl BitswapStore for BitswapBlockstore {
    type Params = libipld::DefaultParams;

    fn contains(&mut self, cid: &Cid) -> anyhow::Result<bool> {
        Blockstore::has(self, cid)
    }

    fn get(&mut self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }

    fn insert(&mut self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        Blockstore::put_keyed(self, block.cid(), block.data())
    }

    fn missing_blocks(&mut self, cid: &Cid) -> anyhow::Result<Vec<Cid>> {
        missing_blocks::<Self, Self::Params>(self, cid)
    }
}
