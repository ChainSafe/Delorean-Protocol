// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::fvm::state::FvmStateParams;
use crate::fvm::store::ReadOnlyBlockstore;
use anyhow::anyhow;
use cid::multihash::{Code, MultihashDigest};
use cid::Cid;
use futures_core::Stream;
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::{load_car, load_car_unchecked, CarHeader};
use fvm_ipld_encoding::{from_slice, CborStore, DAG_CBOR};
use libipld::Ipld;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::StreamExt;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub type BlockHeight = u64;
pub type SnapshotVersion = u32;

/// Taking snapshot of the current blockchain state
pub enum Snapshot<BS> {
    V1(V1Snapshot<BS>),
}

/// Contains the overall metadata for the snapshot
#[derive(Serialize, Deserialize)]
struct SnapshotMetadata {
    version: u8,
    data_root_cid: Cid,
}

/// The streamer that streams the snapshot into (Cid, Vec<u8>) for car file.
type SnapshotStreamer = Box<dyn Send + Unpin + Stream<Item = (Cid, Vec<u8>)>>;

impl<BS> Snapshot<BS>
where
    BS: Blockstore + 'static + Send + Clone,
{
    pub fn new(
        store: BS,
        state_params: FvmStateParams,
        block_height: BlockHeight,
    ) -> anyhow::Result<Self> {
        Ok(Self::V1(V1Snapshot::new(
            store,
            state_params,
            block_height,
        )?))
    }

    pub fn version(&self) -> SnapshotVersion {
        match self {
            Snapshot::V1(_) => 1,
        }
    }

    /// Read the snapshot from file and load all the data into the store
    pub async fn read_car(
        path: impl AsRef<Path>,
        store: BS,
        validate: bool,
    ) -> anyhow::Result<Self> {
        let file = tokio::fs::File::open(path).await?;

        let roots = if validate {
            load_car(&store, file.compat()).await?
        } else {
            load_car_unchecked(&store, file.compat()).await?
        };

        if roots.len() != 1 {
            return Err(anyhow!("invalid snapshot, should have 1 root cid"));
        }

        let metadata_cid = roots[0];
        let metadata = if let Some(metadata) = store.get_cbor::<SnapshotMetadata>(&metadata_cid)? {
            metadata
        } else {
            return Err(anyhow!("invalid snapshot, metadata not found"));
        };

        match metadata.version {
            1 => Ok(Self::V1(V1Snapshot::from_root(
                store,
                metadata.data_root_cid,
            )?)),
            v => Err(anyhow!("unknown snapshot version: {v}")),
        }
    }

    /// Write the snapshot to car file.
    ///
    /// The root cid points to the metadata, i.e `SnapshotMetadata` struct. From the snapshot metadata
    /// one can query the version and root data cid. Based on the version, one can parse the underlying
    /// data of the snapshot from the root cid.
    pub async fn write_car(self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = tokio::fs::File::create(path).await?;

        // derive the metadata for the car file, so that the snapshot version can be recorded.
        let (metadata, snapshot_streamer) = self.into_streamer()?;
        let (metadata_cid, metadata_bytes) = derive_cid(&metadata)?;

        // create the target car header with the metadata cid as the only root
        let car = CarHeader::new(vec![metadata_cid], 1);

        // create the stream to stream all the data into the car file
        let mut streamer =
            tokio_stream::iter(vec![(metadata_cid, metadata_bytes)]).merge(snapshot_streamer);

        let write_task = tokio::spawn(async move {
            let mut write = file.compat_write();
            car.write_stream_async(&mut Pin::new(&mut write), &mut streamer)
                .await
        });

        write_task.await??;

        Ok(())
    }

    fn into_streamer(self) -> anyhow::Result<(SnapshotMetadata, SnapshotStreamer)> {
        match self {
            Snapshot::V1(inner) => {
                let (data_root_cid, streamer) = inner.into_streamer()?;
                Ok((
                    SnapshotMetadata {
                        version: 1,
                        data_root_cid,
                    },
                    streamer,
                ))
            }
        }
    }
}

pub struct V1Snapshot<BS> {
    /// The state tree of the current blockchain
    state_tree: StateTree<ReadOnlyBlockstore<BS>>,
    state_params: FvmStateParams,
    block_height: BlockHeight,
}

pub type BlockStateParams = (FvmStateParams, BlockHeight);

impl<BS> V1Snapshot<BS>
where
    BS: Blockstore + 'static + Send + Clone,
{
    /// Creates a new V2Snapshot struct. Caller ensure store
    pub fn new(
        store: BS,
        state_params: FvmStateParams,
        block_height: BlockHeight,
    ) -> anyhow::Result<Self> {
        let state_tree =
            StateTree::new_from_root(ReadOnlyBlockstore::new(store), &state_params.state_root)?;

        Ok(Self {
            state_tree,
            state_params,
            block_height,
        })
    }

    fn from_root(store: BS, root_cid: Cid) -> anyhow::Result<Self> {
        if let Some((state_params, block_height)) = store.get_cbor::<BlockStateParams>(&root_cid)? {
            let state_tree_root = state_params.state_root;
            Ok(Self {
                state_tree: StateTree::new_from_root(
                    ReadOnlyBlockstore::new(store),
                    &state_tree_root,
                )?,
                state_params,
                block_height,
            })
        } else {
            Err(anyhow!(
                "invalid v1 snapshot, root cid not found: {}",
                root_cid
            ))
        }
    }

    fn into_streamer(self) -> anyhow::Result<(Cid, SnapshotStreamer)> {
        let state_tree_root = self.state_params.state_root;

        let block_state_params = (self.state_params, self.block_height);
        let bytes = fvm_ipld_encoding::to_vec(&block_state_params)?;
        let root_cid = Cid::new_v1(DAG_CBOR, Code::Blake2b256.digest(&bytes));

        let state_tree_streamer =
            StateTreeStreamer::new(state_tree_root, self.state_tree.into_store());
        let root_streamer = tokio_stream::iter(vec![(root_cid, bytes)]);
        let streamer: SnapshotStreamer = Box::new(state_tree_streamer.merge(root_streamer));

        Ok((root_cid, streamer))
    }

    pub fn block_height(&self) -> BlockHeight {
        self.block_height
    }

    pub fn state_params(&self) -> &FvmStateParams {
        &self.state_params
    }
}

#[pin_project::pin_project]
struct StateTreeStreamer<BS> {
    /// The list of cids to pull from the blockstore
    #[pin]
    dfs: VecDeque<Cid>,
    /// The block store
    bs: BS,
}

impl<BS> StateTreeStreamer<BS> {
    pub fn new(state_root_cid: Cid, bs: BS) -> Self {
        let mut dfs = VecDeque::new();
        dfs.push_back(state_root_cid);
        Self { dfs, bs }
    }
}

impl<BS: Blockstore> Stream for StateTreeStreamer<BS> {
    type Item = (Cid, Vec<u8>);

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            let cid = if let Some(cid) = this.dfs.pop_front() {
                cid
            } else {
                return Poll::Ready(None);
            };

            match this.bs.get(&cid) {
                Ok(Some(bytes)) => {
                    // Not all data in the blockstore is traversable, e.g.
                    // Wasm bytecode is inserted as IPLD_RAW here: https://github.com/filecoin-project/builtin-actors-bundler/blob/bf6847b2276ee8e4e17f8336f2eb5ab2fce1d853/src/lib.rs#L54C71-L54C79
                    if cid.codec() == DAG_CBOR {
                        // XXX: Is it okay to panic?
                        let ipld =
                            from_slice::<Ipld>(&bytes).expect("blocktore stores IPLD encoded data");

                        walk_ipld_cids(ipld, &mut this.dfs);
                    }
                    return Poll::Ready(Some((cid, bytes)));
                }
                Ok(None) => {
                    tracing::debug!("cid: {cid:?} has no value in block store, skip");
                    continue;
                }
                Err(e) => {
                    tracing::error!("cannot get from block store: {}", e.to_string());
                    // TODO: consider returning Result, but it won't work with `car.write_stream_async`.
                    return Poll::Ready(None);
                }
            }
        }
    }
}

fn walk_ipld_cids(ipld: Ipld, dfs: &mut VecDeque<Cid>) {
    match ipld {
        Ipld::List(v) => {
            for i in v {
                walk_ipld_cids(i, dfs);
            }
        }
        Ipld::Map(map) => {
            for v in map.into_values() {
                walk_ipld_cids(v, dfs);
            }
        }
        Ipld::Link(cid) => dfs.push_back(cid),
        _ => {}
    }
}

fn derive_cid<T: Serialize>(t: &T) -> anyhow::Result<(Cid, Vec<u8>)> {
    let bytes = fvm_ipld_encoding::to_vec(&t)?;
    let cid = Cid::new_v1(DAG_CBOR, Code::Blake2b256.digest(&bytes));
    Ok((cid, bytes))
}

#[cfg(test)]
mod tests {
    use crate::fvm::state::snapshot::{Snapshot, StateTreeStreamer};
    use crate::fvm::state::FvmStateParams;
    use crate::fvm::store::memory::MemoryBlockstore;
    use crate::fvm::store::ReadOnlyBlockstore;
    use cid::Cid;
    use fendermint_vm_core::Timestamp;
    use futures_util::StreamExt;
    use fvm::state_tree::{ActorState, StateTree};
    use fvm_ipld_blockstore::Blockstore;
    use fvm_shared::state::StateTreeVersion;
    use fvm_shared::version::NetworkVersion;
    use quickcheck::{Arbitrary, Gen};
    use std::collections::VecDeque;

    fn prepare_state_tree(items: u64) -> (Cid, StateTree<MemoryBlockstore>) {
        let store = MemoryBlockstore::new();
        let mut state_tree = StateTree::new(store, StateTreeVersion::V5).unwrap();
        let mut gen = Gen::new(16);

        for i in 1..=items {
            let state = ActorState::arbitrary(&mut gen);
            state_tree.set_actor(i, state);
        }
        let root_cid = state_tree.flush().unwrap();
        (root_cid, state_tree)
    }

    fn assert_tree2_contains_tree1<Store1: Blockstore, Store2: Blockstore>(
        tree1: &StateTree<Store1>,
        tree2: &StateTree<Store2>,
    ) {
        tree1
            .for_each(|addr, state| {
                let r = tree2.get_actor_by_address(&addr);
                if r.is_err() {
                    panic!("addr: {addr:?} does not exists in tree 2");
                }

                if let Some(target_state) = r.unwrap() {
                    assert_eq!(target_state, *state);
                } else {
                    panic!("missing address: {addr:?}");
                }
                Ok(())
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_streamer() {
        let (root_cid, state_tree) = prepare_state_tree(100);
        let bs = state_tree.into_store();
        let mut stream = StateTreeStreamer {
            dfs: VecDeque::from(vec![root_cid]),
            bs: bs.clone(),
        };

        let new_bs = MemoryBlockstore::new();
        while let Some((cid, bytes)) = stream.next().await {
            new_bs.put_keyed(&cid, &bytes).unwrap();
        }

        let new_state_tree = StateTree::new_from_root(new_bs, &root_cid).unwrap();
        let old_state_tree = StateTree::new_from_root(bs, &root_cid).unwrap();

        assert_tree2_contains_tree1(&old_state_tree, &new_state_tree);
        assert_tree2_contains_tree1(&new_state_tree, &old_state_tree);
    }

    #[tokio::test]
    async fn test_car() {
        let (state_root, state_tree) = prepare_state_tree(100);
        let state_params = FvmStateParams {
            state_root,
            timestamp: Timestamp(100),
            network_version: NetworkVersion::V1,
            base_fee: Default::default(),
            circ_supply: Default::default(),
            chain_id: 1024,
            power_scale: 0,
            app_version: 0,
        };
        let block_height = 2048;

        let bs = state_tree.into_store();
        let db = ReadOnlyBlockstore::new(bs.clone());
        let snapshot = Snapshot::new(db, state_params.clone(), block_height).unwrap();

        let tmp_file = tempfile::NamedTempFile::new().unwrap();
        let r = snapshot.write_car(tmp_file.path()).await;
        assert!(r.is_ok());

        let new_store = MemoryBlockstore::new();
        let Snapshot::V1(loaded_snapshot) = Snapshot::read_car(tmp_file.path(), new_store, true)
            .await
            .unwrap();

        assert_eq!(state_params, loaded_snapshot.state_params);
        assert_eq!(block_height, loaded_snapshot.block_height);
        assert_tree2_contains_tree1(
            &StateTree::new_from_root(bs, &loaded_snapshot.state_params.state_root).unwrap(),
            &loaded_snapshot.state_tree,
        );
    }
}
