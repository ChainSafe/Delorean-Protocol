// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{future::Future, time::Duration};

use async_stm::{atomically, queues::TQueueLike};
use ipc_api::subnet_id::SubnetID;
use ipc_ipld_resolver::Resolver;

use crate::pool::{ResolveQueue, ResolveTask};

/// The IPLD Resolver takes resolution tasks from the [ResolvePool] and
/// uses the [ipc_ipld_resolver] to fetch the content from subnets.
pub struct IpldResolver<C> {
    client: C,
    queue: ResolveQueue,
    retry_delay: Duration,
    own_subnet_id: SubnetID,
}

impl<C> IpldResolver<C>
where
    C: Resolver + Clone + Send + 'static,
{
    pub fn new(
        client: C,
        queue: ResolveQueue,
        retry_delay: Duration,
        own_subnet_id: SubnetID,
    ) -> Self {
        Self {
            client,
            queue,
            retry_delay,
            own_subnet_id,
        }
    }

    /// Start taking tasks from the resolver pool and resolving them using the IPLD Resolver.
    pub async fn run(self) {
        loop {
            let (task, use_own_subnet) = atomically(|| {
                let task = self.queue.read()?;
                let use_own_subnet = task.use_own_subnet()?;
                Ok((task, use_own_subnet))
            })
            .await;

            start_resolve(
                task,
                self.client.clone(),
                self.queue.clone(),
                self.retry_delay,
                if use_own_subnet {
                    Some(self.own_subnet_id.clone())
                } else {
                    None
                },
            );
        }
    }
}

/// Run task resolution in the background, so as not to block items from other
/// subnets being tried.
fn start_resolve<C>(
    task: ResolveTask,
    client: C,
    queue: ResolveQueue,
    retry_delay: Duration,
    own_subnet_id: Option<SubnetID>,
) where
    C: Resolver + Send + 'static,
{
    tokio::spawn(async move {
        let from_theirs = client.resolve(task.cid(), task.subnet_id());
        let from_own = own_subnet_id.map(|subnet_id| client.resolve(task.cid(), subnet_id));

        let (theirs, own) = tokio::join!(from_theirs, future_opt(from_own));

        let err = match (theirs, own) {
            (Err(e), _) => {
                tracing::error!(error = e.to_string(), "failed to submit resolution task");
                // The service is no longer listening, we might as well stop taking new tasks from the queue.
                // By not quitting we should see this error every time there is a new task, which is at least is a constant reminder.
                return;
            }
            (Ok(Ok(())), _) | (_, Some(Ok(Ok(())))) => None,
            (Ok(Err(e)), _) => Some(e),
        };

        match err {
            None => {
                tracing::debug!(cid = ?task.cid(), "content resolved");
                atomically(|| task.set_resolved()).await;
            }
            Some(e) => {
                tracing::error!(
                    cid = ?task.cid(),
                    error = e.to_string(),
                    "content resolution failed; retrying later"
                );
                schedule_retry(task, queue, retry_delay);
            }
        }
    });
}

/// Run a future option, returning the optional result.
async fn future_opt<F, T>(f: Option<F>) -> Option<T>
where
    F: Future<Output = T>,
{
    match f {
        None => None,
        Some(f) => Some(f.await),
    }
}

/// Part of error handling.
///
/// In our case we enqueued the task from transaction processing,
/// which will not happen again, so there is no point further
/// propagating this error back to the sender to deal with.
/// Rather, we should retry until we can conclude whether it will
/// ever complete. Some errors raised by the service are transitive,
/// such as having no peers currently, but that might change.
///
/// For now, let's retry the same task later.
fn schedule_retry(task: ResolveTask, queue: ResolveQueue, retry_delay: Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(retry_delay).await;
        tracing::debug!(cid = ?task.cid(), "retrying content resolution");
        atomically(move || queue.write(task.clone())).await;
    });
}
