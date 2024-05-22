// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::HashSet, hash::Hash};

use async_stm::{
    queues::{tchan::TChan, TQueueLike},
    Stm, TVar,
};
use cid::Cid;
use ipc_api::subnet_id::SubnetID;

/// CIDs we need to resolve from a specific source subnet, or our own.
pub type ResolveKey = (SubnetID, Cid);

/// Ongoing status of a resolution.
///
/// The status also keeps track of which original items mapped to the same resolution key.
/// These could be for example checkpoint of the same data with slightly different signatories.
/// Once resolved, they all become available at the same time.
#[derive(Clone)]
pub struct ResolveStatus<T> {
    /// Indicate whether the content has been resolved.
    ///
    /// If needed we can expand on this to include failure states.
    is_resolved: TVar<bool>,
    /// Indicate whether our peers in our own subnet should be contacted.
    use_own_subnet: TVar<bool>,
    /// The collection of items that all resolve to the same root CID and subnet.
    items: TVar<im::HashSet<T>>,
}

impl<T> ResolveStatus<T>
where
    T: Clone + Hash + Eq + PartialEq + Sync + Send + 'static,
{
    pub fn new(item: T, use_own_subnet: bool) -> Self {
        let mut items = im::HashSet::new();
        items.insert(item);
        Self {
            is_resolved: TVar::new(false),
            use_own_subnet: TVar::new(use_own_subnet),
            items: TVar::new(items),
        }
    }

    pub fn is_resolved(&self) -> Stm<bool> {
        self.is_resolved.read_clone()
    }
}

/// Tasks emitted by the pool for background resolution.
#[derive(Clone)]
pub struct ResolveTask {
    /// Content to resolve.
    key: ResolveKey,
    /// Flag to flip when the task is done.
    is_resolved: TVar<bool>,
    /// Flag to flip if consensus reached a state on its own
    /// where the majority of our own peers should have an item.
    use_own_subnet: TVar<bool>,
}

impl ResolveTask {
    pub fn cid(&self) -> Cid {
        self.key.1
    }

    pub fn subnet_id(&self) -> SubnetID {
        self.key.0.clone()
    }

    pub fn set_resolved(&self) -> Stm<()> {
        self.is_resolved.write(true)
    }

    pub fn use_own_subnet(&self) -> Stm<bool> {
        self.use_own_subnet.read_clone()
    }
}

pub type ResolveQueue = TChan<ResolveTask>;

/// A data structure used to communicate resolution requirements and outcomes
/// between the resolver running in the background and the application waiting
/// for the results.
///
/// It is designed to resolve a single CID from a single subnet, per item,
/// with the possibility of multiple items mapping to the same CID.
///
/// If items needed to have multiple CIDs, the completion of all resolutions
/// culminating in the availability of the item, then we have to refactor this
/// component to track dependencies in a different way. For now I am assuming
/// that we can always design our messages in a way that there is a single root.
/// We can also use technical wrappers to submit the same item under different
/// guises and track the completion elsewhere.
#[derive(Clone, Default)]
pub struct ResolvePool<T>
where
    T: Clone + Sync + Send + 'static,
{
    /// The resolution status of each item.
    items: TVar<im::HashMap<ResolveKey, ResolveStatus<T>>>,
    /// Items queued for resolution.
    queue: ResolveQueue,
}

impl<T> ResolvePool<T>
where
    for<'a> ResolveKey: From<&'a T>,
    T: Sync + Send + Clone + Hash + Eq + PartialEq + 'static,
{
    pub fn new() -> Self {
        Self {
            items: Default::default(),
            queue: Default::default(),
        }
    }

    /// Queue to consume for task items.
    ///
    /// Exposed as-is to allow re-queueing items.
    pub fn queue(&self) -> ResolveQueue {
        self.queue.clone()
    }

    /// Add an item to the resolution targets.
    ///
    /// If the item is new, enqueue it from background resolution, otherwise just return its existing status.
    pub fn add(&self, item: T, use_own_subnet: bool) -> Stm<ResolveStatus<T>> {
        let key = ResolveKey::from(&item);
        let mut items = self.items.read_clone()?;

        if items.contains_key(&key) {
            let status = items.get(&key).cloned().unwrap();
            status.use_own_subnet.update(|u| u || use_own_subnet)?;
            status.items.update_mut(|items| {
                items.insert(item);
            })?;
            Ok(status)
        } else {
            let status = ResolveStatus::new(item, use_own_subnet);
            items.insert(key.clone(), status.clone());
            self.items.write(items)?;
            self.queue.write(ResolveTask {
                key,
                is_resolved: status.is_resolved.clone(),
                use_own_subnet: status.use_own_subnet.clone(),
            })?;
            Ok(status)
        }
    }

    /// Return the status of an item. It can be queried for completion.
    pub fn get_status(&self, item: &T) -> Stm<Option<ResolveStatus<T>>> {
        let key = ResolveKey::from(item);
        Ok(self.items.read()?.get(&key).cloned())
    }

    /// Collect resolved items, ready for execution.
    ///
    /// The items collected are not removed, in case they need to be proposed again.
    pub fn collect_resolved(&self) -> Stm<HashSet<T>> {
        let mut resolved = HashSet::new();
        let items = self.items.read()?;
        for item in items.values() {
            if item.is_resolved()? {
                let items = item.items.read()?;
                resolved.extend(items.iter().cloned());
            }
        }
        Ok(resolved)
    }

    /// Await the next item to be resolved.
    pub fn next(&self) -> Stm<ResolveTask> {
        self.queue.read()
    }

    // TODO #197: Implement methods to remove executed items.
}

#[cfg(test)]
mod tests {
    use async_stm::{atomically, queues::TQueueLike};
    use cid::Cid;
    use ipc_api::subnet_id::SubnetID;

    #[derive(Clone, Hash, Eq, PartialEq, Debug)]
    struct TestItem {
        subnet_id: SubnetID,
        cid: Cid,
    }

    impl TestItem {
        pub fn dummy(root_id: u64) -> Self {
            Self {
                subnet_id: SubnetID::new_root(root_id),
                cid: Cid::default(),
            }
        }
    }

    impl From<&TestItem> for ResolveKey {
        fn from(value: &TestItem) -> Self {
            (value.subnet_id.clone(), value.cid)
        }
    }

    use super::{ResolveKey, ResolvePool};

    #[tokio::test]
    async fn add_new_item() {
        let pool = ResolvePool::new();
        let item = TestItem::dummy(0);

        atomically(|| pool.add(item.clone(), false)).await;
        atomically(|| {
            assert!(pool.get_status(&item)?.is_some());
            assert!(!pool.queue.is_empty()?);
            assert_eq!(pool.queue.read()?.key, ResolveKey::from(&item));
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn add_existing_item() {
        let pool = ResolvePool::new();
        let item = TestItem::dummy(0);

        // Add once.
        atomically(|| pool.add(item.clone(), false)).await;

        // Consume it from the queue.
        atomically(|| {
            assert!(!pool.queue.is_empty()?);
            let _ = pool.queue.read()?;
            Ok(())
        })
        .await;

        // Add again.
        atomically(|| pool.add(item.clone(), true)).await;

        // Should not be queued a second time.
        atomically(|| {
            let status = pool.get_status(&item)?;
            assert!(status.is_some());
            assert!(status.unwrap().use_own_subnet.read_clone()?);
            assert!(pool.queue.is_empty()?);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn get_status() {
        let pool = ResolvePool::new();
        let item = TestItem::dummy(0);

        let status1 = atomically(|| pool.add(item.clone(), false)).await;
        let status2 = atomically(|| pool.get_status(&item))
            .await
            .expect("status exists");

        // Complete the item.
        atomically(|| {
            assert!(!pool.queue.is_empty()?);
            let task = pool.queue.read()?;
            task.is_resolved.write(true)
        })
        .await;

        // Check status.
        atomically(|| {
            assert!(status1.items.read()?.contains(&item));
            assert!(status1.is_resolved()?);
            assert!(status2.is_resolved()?);
            Ok(())
        })
        .await;
    }

    #[tokio::test]
    async fn collect_resolved() {
        let pool = ResolvePool::new();
        let item = TestItem::dummy(0);

        atomically(|| {
            let status = pool.add(item.clone(), false)?;
            status.is_resolved.write(true)?;

            let resolved1 = pool.collect_resolved()?;
            let resolved2 = pool.collect_resolved()?;
            assert_eq!(resolved1, resolved2);
            assert!(resolved1.contains(&item));
            Ok(())
        })
        .await;
    }
}
