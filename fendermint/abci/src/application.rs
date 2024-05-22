// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use async_trait::async_trait;
use futures::future::FutureExt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tendermint::abci::{request, response, Request, Response};
use tower::Service;
use tower_abci::BoxError;

use crate::util::take_until_max_size;

/// Allow returning a result from the methods, so the [`Application`]
/// implementation doesn't have to be full of `.expect("failed...")`
/// or `.unwrap()` calls. It is still good practice to use for example
/// `anyhow::Context` to provide better error feedback.
///
/// If an error is returned, the [`tower_abci::Service`] will handle
/// it by crashing at the moment.
pub type AbciResult<T> = std::result::Result<T, BoxError>;

/// Asynchronous equivalent of of [tendermint_abci::Application].
///
/// See the [spec](https://github.com/tendermint/tendermint/blob/v0.37.0-rc2/spec/abci) for the expected behaviour.
#[allow(unused_variables)]
#[async_trait]
pub trait Application {
    /// Echo back the same message as provided in the request.
    async fn echo(&self, request: request::Echo) -> AbciResult<response::Echo> {
        Ok(response::Echo {
            message: request.message,
        })
    }

    /// Provide information about the ABCI application.
    async fn info(&self, request: request::Info) -> AbciResult<response::Info> {
        Ok(Default::default())
    }

    /// Called once upon genesis.
    async fn init_chain(&self, request: request::InitChain) -> AbciResult<response::InitChain> {
        Ok(Default::default())
    }

    /// Query the application for data at the current or past height.
    async fn query(&self, request: request::Query) -> AbciResult<response::Query> {
        Ok(Default::default())
    }

    /// Check the given transaction before putting it into the local mempool.
    async fn check_tx(&self, request: request::CheckTx) -> AbciResult<response::CheckTx> {
        Ok(Default::default())
    }

    /// Opportunity for the application to modify the proposed transactions.
    ///
    /// The application must copy the transactions it wants to propose into the response and respect the size restrictions.
    ///
    /// See the [spec](https://github.com/tendermint/tendermint/tree/v0.37.0-rc2/spec/abci#prepareproposal).
    async fn prepare_proposal(
        &self,
        request: request::PrepareProposal,
    ) -> AbciResult<response::PrepareProposal> {
        let txs = take_until_max_size(request.txs, request.max_tx_bytes.try_into().unwrap());

        Ok(response::PrepareProposal { txs })
    }

    /// Opportunity for the application to inspect the proposal before voting on it.
    ///
    /// The application should accept the proposal unless there's something wrong with it.
    ///
    /// See the [spec](https://github.com/tendermint/tendermint/tree/v0.37.0-rc2/spec/abci#processproposal).
    async fn process_proposal(
        &self,
        request: request::ProcessProposal,
    ) -> AbciResult<response::ProcessProposal> {
        Ok(response::ProcessProposal::Accept)
    }

    /// Signals the beginning of a new block, prior to any `DeliverTx` calls.
    async fn begin_block(&self, request: request::BeginBlock) -> AbciResult<response::BeginBlock> {
        Ok(Default::default())
    }

    /// Apply a transaction to the application's state.
    async fn deliver_tx(&self, request: request::DeliverTx) -> AbciResult<response::DeliverTx> {
        Ok(Default::default())
    }

    /// Signals the end of a block.
    async fn end_block(&self, request: request::EndBlock) -> AbciResult<response::EndBlock> {
        Ok(Default::default())
    }

    /// Commit the current state at the current height.
    async fn commit(&self) -> AbciResult<response::Commit> {
        Ok(Default::default())
    }

    /// Used during state sync to discover available snapshots on peers.
    async fn list_snapshots(&self) -> AbciResult<response::ListSnapshots> {
        Ok(Default::default())
    }

    /// Called when bootstrapping the node using state sync.
    async fn offer_snapshot(
        &self,
        request: request::OfferSnapshot,
    ) -> AbciResult<response::OfferSnapshot> {
        Ok(Default::default())
    }

    /// Used during state sync to retrieve chunks of snapshots from peers.
    async fn load_snapshot_chunk(
        &self,
        request: request::LoadSnapshotChunk,
    ) -> AbciResult<response::LoadSnapshotChunk> {
        Ok(Default::default())
    }

    /// Apply the given snapshot chunk to the application's state.
    async fn apply_snapshot_chunk(
        &self,
        request: request::ApplySnapshotChunk,
    ) -> AbciResult<response::ApplySnapshotChunk> {
        Ok(Default::default())
    }
}

/// Wrapper to adapt an `Application` to a `tower::Service`.
pub struct ApplicationService<A: Application + Sync + Send + Clone + 'static>(pub A);

impl<A> Service<Request> for ApplicationService<A>
where
    A: Application + Sync + Send + Clone + 'static,
{
    type Response = Response;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Response, BoxError>> + Send + 'static>>;

    /// At this level the application is always ready to receive requests.
    /// Throttling is handled in the layers added on top of it.
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        // Must make sure this is a cheap clone, required so the app can be moved into the async boxed future.
        // See https://tokio.rs/blog/2021-05-14-inventing-the-service-trait
        // The alternative is to perform the operation synchronously right here,
        // but if we use the `tower_abci::buffer4::Worker` that means nothing else
        // get processed during that time.
        let app = self.0.clone();

        // Another trick to avoid any subtle bugs is the mem::replace.
        // See https://github.com/tower-rs/tower/issues/547
        let app: A = std::mem::replace(&mut self.0, app);

        // Because this is async, make sure the `Consensus` service is wrapped in a concurrency limiting Tower layer.
        let res = async move {
            let res = match req {
                Request::Echo(r) => Response::Echo(log_error(app.echo(r).await)?),
                Request::Info(r) => Response::Info(log_error(app.info(r).await)?),
                Request::InitChain(r) => Response::InitChain(log_error(app.init_chain(r).await)?),
                Request::Query(r) => Response::Query(log_error(app.query(r).await)?),
                Request::CheckTx(r) => Response::CheckTx(log_error(app.check_tx(r).await)?),
                Request::PrepareProposal(r) => {
                    Response::PrepareProposal(log_error(app.prepare_proposal(r).await)?)
                }
                Request::ProcessProposal(r) => {
                    Response::ProcessProposal(log_error(app.process_proposal(r).await)?)
                }
                Request::BeginBlock(r) => {
                    Response::BeginBlock(log_error(app.begin_block(r).await)?)
                }
                Request::DeliverTx(r) => Response::DeliverTx(log_error(app.deliver_tx(r).await)?),
                Request::EndBlock(r) => Response::EndBlock(log_error(app.end_block(r).await)?),
                Request::Commit => Response::Commit(log_error(app.commit().await)?),
                Request::ListSnapshots => {
                    Response::ListSnapshots(log_error(app.list_snapshots().await)?)
                }
                Request::OfferSnapshot(r) => {
                    Response::OfferSnapshot(log_error(app.offer_snapshot(r).await)?)
                }
                Request::LoadSnapshotChunk(r) => {
                    Response::LoadSnapshotChunk(log_error(app.load_snapshot_chunk(r).await)?)
                }
                Request::ApplySnapshotChunk(r) => {
                    Response::ApplySnapshotChunk(log_error(app.apply_snapshot_chunk(r).await)?)
                }
                Request::Flush => panic!("Flush should be handled by the Server!"),
            };
            Ok(res)
        };
        res.boxed()
    }
}

fn log_error<T>(res: AbciResult<T>) -> AbciResult<T> {
    if let Err(ref e) = res {
        tracing::error!("failed to execute ABCI request: {e:#}");
    }
    res
}
