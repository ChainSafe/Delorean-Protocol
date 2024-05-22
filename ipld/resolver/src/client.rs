// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use anyhow::anyhow;
use async_trait::async_trait;
use ipc_api::subnet_id::SubnetID;
use libipld::Cid;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::{
    service::{Request, ResolveResult},
    vote_record::SignedVoteRecord,
};

/// A facade to the [`Service`] to provide a nicer interface than message passing would allow on its own.
#[derive(Clone)]
pub struct Client<V> {
    request_tx: UnboundedSender<Request<V>>,
}

impl<V> Client<V> {
    pub(crate) fn new(request_tx: UnboundedSender<Request<V>>) -> Self {
        Self { request_tx }
    }

    /// Send a request to the [`Service`], unless it has stopped listening.
    fn send_request(&self, req: Request<V>) -> anyhow::Result<()> {
        self.request_tx
            .send(req)
            .map_err(|_| anyhow!("disconnected"))
    }

    /// Set the complete list of subnets currently supported by this node.
    pub fn set_provided_subnets(&self, subnet_ids: Vec<SubnetID>) -> anyhow::Result<()> {
        let req = Request::SetProvidedSubnets(subnet_ids);
        self.send_request(req)
    }

    /// Add a subnet supported by this node.
    pub fn add_provided_subnet(&self, subnet_id: SubnetID) -> anyhow::Result<()> {
        let req = Request::AddProvidedSubnet(subnet_id);
        self.send_request(req)
    }

    /// Remove a subnet no longer supported by this node.
    pub fn remove_provided_subnet(&self, subnet_id: SubnetID) -> anyhow::Result<()> {
        let req = Request::RemoveProvidedSubnet(subnet_id);
        self.send_request(req)
    }

    /// Add a subnet we know really exist and we are interested in them.
    pub fn pin_subnet(&self, subnet_id: SubnetID) -> anyhow::Result<()> {
        let req = Request::PinSubnet(subnet_id);
        self.send_request(req)
    }

    /// Unpin a we are no longer interested in.
    pub fn unpin_subnet(&self, subnet_id: SubnetID) -> anyhow::Result<()> {
        let req = Request::UnpinSubnet(subnet_id);
        self.send_request(req)
    }

    /// Update the rate limit based on new projections for the same timeframe
    /// the `content::Behaviour` was originally configured with. This can be
    /// used if we can't come up with a good estimate for the amount of data
    /// we have to serve from the subnets we participate in, but we can adjust
    /// them on the fly based on what we observe on chain.
    pub fn update_rate_limit(&self, bytes: u32) -> anyhow::Result<()> {
        let req = Request::UpdateRateLimit(bytes);
        self.send_request(req)
    }

    /// Publish a signed vote into a topic based on its subnet.
    pub fn publish_vote(&self, vote: SignedVoteRecord<V>) -> anyhow::Result<()> {
        let req = Request::PublishVote(Box::new(vote));
        self.send_request(req)
    }

    /// Publish pre-emptively to a subnet that agents in the parent subnet
    /// would be subscribed to if they are interested in receiving data
    /// before they would have to use [`Client::resolve`] instead.
    pub fn publish_preemptive(&self, subnet_id: SubnetID, data: Vec<u8>) -> anyhow::Result<()> {
        let req = Request::PublishPreemptive(subnet_id, data);
        self.send_request(req)
    }
}

/// Trait to limit the capabilities to resolving CIDs.
#[async_trait]
pub trait Resolver {
    /// Send a CID for resolution from a specific subnet, await its completion,
    /// then return the result, to be inspected by the caller.
    ///
    /// Upon success, the data should be found in the store.
    async fn resolve(&self, cid: Cid, subnet_id: SubnetID) -> anyhow::Result<ResolveResult>;
}

#[async_trait]
impl<V> Resolver for Client<V>
where
    V: Sync + Send + 'static,
{
    /// Send a CID for resolution from a specific subnet, await its completion,
    /// then return the result, to be inspected by the caller.
    ///
    /// Upon success, the data should be found in the store.
    async fn resolve(&self, cid: Cid, subnet_id: SubnetID) -> anyhow::Result<ResolveResult> {
        let (tx, rx) = oneshot::channel();
        let req = Request::Resolve(cid, subnet_id, tx);
        self.send_request(req)?;
        let res = rx.await?;
        Ok(res)
    }
}
