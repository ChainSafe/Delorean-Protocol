// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;
use std::marker::PhantomData;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use fendermint_vm_message::chain::ChainMessage;
use tendermint::abci::response::DeliverTx;
use tendermint::block::Height;
use tendermint_rpc::{endpoint::abci_query::AbciQuery, Client, HttpClient, Scheme, Url};
use tendermint_rpc::{WebSocketClient, WebSocketClientDriver, WebSocketClientUrl};

use fendermint_vm_message::query::{FvmQuery, FvmQueryHeight};

use crate::message::SignedMessageFactory;
use crate::query::QueryClient;
use crate::tx::{
    AsyncResponse, BoundClient, CommitResponse, SyncResponse, TxAsync, TxClient, TxCommit, TxSync,
};

// Retrieve the proxy URL with precedence:
// 1. If supplied, that's the proxy URL used.
// 2. If not supplied, but environment variable HTTP_PROXY or HTTPS_PROXY are
//    supplied, then use the appropriate variable for the URL in question.
//
// Copied from `tendermint_rpc`.
fn get_http_proxy_url(url_scheme: Scheme, proxy_url: Option<Url>) -> anyhow::Result<Option<Url>> {
    match proxy_url {
        Some(u) => Ok(Some(u)),
        None => match url_scheme {
            Scheme::Http => std::env::var("HTTP_PROXY").ok(),
            Scheme::Https => std::env::var("HTTPS_PROXY")
                .ok()
                .or_else(|| std::env::var("HTTP_PROXY").ok()),
            _ => {
                if std::env::var("HTTP_PROXY").is_ok() || std::env::var("HTTPS_PROXY").is_ok() {
                    tracing::warn!(
                        "Ignoring HTTP proxy environment variables for non-HTTP client connection"
                    );
                }
                None
            }
        }
        .map(|u| u.parse::<Url>().map_err(|e| anyhow!(e)))
        .transpose(),
    }
}

/// Create a Tendermint HTTP client.
pub fn http_client(url: Url, proxy_url: Option<Url>) -> anyhow::Result<HttpClient> {
    let proxy_url = get_http_proxy_url(url.scheme(), proxy_url)?;
    let client = match proxy_url {
        Some(proxy_url) => {
            tracing::debug!(
                "Using HTTP client with proxy {} to submit request to {}",
                proxy_url,
                url
            );
            HttpClient::new_with_proxy(url, proxy_url)?
        }
        None => {
            tracing::debug!("Using HTTP client to submit request to: {}", url);
            HttpClient::new(url)?
        }
    };
    Ok(client)
}

/// Create a Tendermint WebSocket client.
///
/// The caller must start the driver in a background task.
pub async fn ws_client<U>(url: U) -> anyhow::Result<(WebSocketClient, WebSocketClientDriver)>
where
    U: TryInto<WebSocketClientUrl, Error = tendermint_rpc::Error> + Display + Clone,
{
    // TODO: Doesn't handle proxy.
    tracing::debug!("Using WS client to submit request to: {}", url);

    let (client, driver) = WebSocketClient::new(url.clone())
        .await
        .with_context(|| format!("failed to create WS client to: {}", url))?;

    Ok((client, driver))
}

/// Unauthenticated Fendermint client.
#[derive(Clone)]
pub struct FendermintClient<C = HttpClient> {
    inner: C,
}

impl<C> FendermintClient<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }

    /// Attach a message factory to the client.
    pub fn bind(self, message_factory: SignedMessageFactory) -> BoundFendermintClient<C> {
        BoundFendermintClient::new(self.inner, message_factory)
    }
}

impl FendermintClient<HttpClient> {
    pub fn new_http(url: Url, proxy_url: Option<Url>) -> anyhow::Result<Self> {
        let inner = http_client(url, proxy_url)?;
        Ok(Self { inner })
    }
}

/// Get to the underlying Tendermint client if necessary, for example to query the state of transactions.
pub trait TendermintClient<C> {
    /// The underlying Tendermint client.
    fn underlying(&self) -> &C;
    fn into_underlying(self) -> C;
}

impl<C> TendermintClient<C> for FendermintClient<C> {
    fn underlying(&self) -> &C {
        &self.inner
    }

    fn into_underlying(self) -> C {
        self.inner
    }
}

#[async_trait]
impl<C> QueryClient for FendermintClient<C>
where
    C: Client + Sync + Send,
{
    async fn perform(&self, query: FvmQuery, height: FvmQueryHeight) -> anyhow::Result<AbciQuery> {
        perform_query(&self.inner, query, height).await
    }
}

/// Fendermint client capable of signing transactions.
pub struct BoundFendermintClient<C = HttpClient> {
    inner: C,
    message_factory: SignedMessageFactory,
}

impl<C> BoundFendermintClient<C> {
    pub fn new(inner: C, message_factory: SignedMessageFactory) -> Self {
        Self {
            inner,
            message_factory,
        }
    }
}

impl<C> BoundClient for BoundFendermintClient<C> {
    fn message_factory_mut(&mut self) -> &mut SignedMessageFactory {
        &mut self.message_factory
    }
}

impl<C> TendermintClient<C> for BoundFendermintClient<C> {
    fn underlying(&self) -> &C {
        &self.inner
    }
    fn into_underlying(self) -> C {
        self.inner
    }
}

#[async_trait]
impl<C> QueryClient for BoundFendermintClient<C>
where
    C: Client + Sync + Send,
{
    async fn perform(&self, query: FvmQuery, height: FvmQueryHeight) -> anyhow::Result<AbciQuery> {
        perform_query(&self.inner, query, height).await
    }
}

#[async_trait]
impl<C> TxClient<TxAsync> for BoundFendermintClient<C>
where
    C: Client + Sync + Send,
{
    async fn perform<F, T>(&self, msg: ChainMessage, _f: F) -> anyhow::Result<AsyncResponse<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
    {
        let data = SignedMessageFactory::serialize(&msg)?;
        let response = self
            .inner
            .broadcast_tx_async(data)
            .await
            .context("broadcast_tx_async failed")?;
        let response = AsyncResponse {
            response,
            return_data: PhantomData,
        };
        Ok(response)
    }
}

#[async_trait]
impl<C> TxClient<TxSync> for BoundFendermintClient<C>
where
    C: Client + Sync + Send,
{
    async fn perform<F, T>(
        &self,
        msg: ChainMessage,
        _f: F,
    ) -> anyhow::Result<crate::tx::SyncResponse<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
    {
        let data = SignedMessageFactory::serialize(&msg)?;
        let response = self
            .inner
            .broadcast_tx_sync(data)
            .await
            .context("broadcast_tx_sync failed")?;
        let response = SyncResponse {
            response,
            return_data: PhantomData,
        };
        Ok(response)
    }
}

#[async_trait]
impl<C> TxClient<TxCommit> for BoundFendermintClient<C>
where
    C: Client + Sync + Send,
{
    async fn perform<F, T>(
        &self,
        msg: ChainMessage,
        f: F,
    ) -> anyhow::Result<crate::tx::CommitResponse<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
    {
        let data = SignedMessageFactory::serialize(&msg)?;
        let response = self
            .inner
            .broadcast_tx_commit(data)
            .await
            .context("broadcast_tx_commit failed")?;
        // We have a fully `DeliverTx` with default fields even if `CheckTx` indicates failure.
        let return_data = if response.check_tx.code.is_err() || response.deliver_tx.code.is_err() {
            None
        } else {
            let return_data =
                f(&response.deliver_tx).context("error decoding data from deliver_tx in commit")?;
            Some(return_data)
        };
        let response = CommitResponse {
            response,
            return_data,
        };
        Ok(response)
    }
}

async fn perform_query<C>(
    client: &C,
    query: FvmQuery,
    height: FvmQueryHeight,
) -> anyhow::Result<AbciQuery>
where
    C: Client + Sync + Send,
{
    tracing::debug!(?query, ?height, "perform ABCI query");
    let data = fvm_ipld_encoding::to_vec(&query).context("failed to encode query")?;
    let height: u64 = height.into();
    let height = Height::try_from(height).context("failed to conver to Height")?;

    // This is how we'd call it, but here we're trying to debug what's going on using
    // the `perform` method below with a request that prints the response if it fails
    // to deserialize for any reason.
    // let res = client
    //     .abci_query(None, data, Some(height), false)
    //     .await
    //     .context("abci query failed")?;

    let req = tendermint_rpc::endpoint::abci_query::Request::new(None, data, Some(height), false);

    let res = client
        .perform(debug::DebugRequest(req))
        .await
        .context("abci query failed")?
        .0
        .response;

    Ok(res)
}

mod debug {
    use serde::{Deserialize, Serialize};
    use tendermint_rpc as trpc;

    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct DebugRequest<R>(pub R);

    #[derive(Serialize, Deserialize)]
    pub struct DebugResponse<R>(pub R);

    impl<R> trpc::Request for DebugRequest<R>
    where
        R: trpc::Request,
    {
        type Response = DebugResponse<R::Response>;
    }

    impl<R> trpc::request::RequestMessage for DebugRequest<R>
    where
        R: trpc::request::RequestMessage,
    {
        fn method(&self) -> trpc::Method {
            self.0.method()
        }
    }

    impl<R> trpc::SimpleRequest for DebugRequest<R>
    where
        R: trpc::SimpleRequest,
    {
        type Output = Self::Response;
    }

    impl<R> trpc::Response for DebugResponse<R>
    where
        R: trpc::Response,
    {
        fn from_string(response: impl AsRef<[u8]>) -> Result<Self, trpc::Error> {
            let wrapper: Result<trpc::response::Wrapper<Self>, trpc::Error> =
                serde_json::from_slice(response.as_ref()).map_err(trpc::Error::serde);

            let response_body = || String::from_utf8_lossy(response.as_ref()).to_string();

            match wrapper {
                Err(e) => {
                    tracing::error!(
                        error = e.to_string(),
                        response = response_body(),
                        "failed to parse JSON-RPC response"
                    );
                    Err(e)
                }
                Ok(wrapper) => match wrapper.into_result() {
                    Err(e) => {
                        tracing::error!(
                            error = e.to_string(),
                            response = response_body(),
                            "error from JSON-RPC"
                        );
                        Err(e)
                    }
                    Ok(response) => Ok(response),
                },
            }
        }

        fn from_reader(reader: impl std::io::prelude::Read) -> Result<Self, trpc::Error> {
            let wrapper: trpc::response::Wrapper<Self> =
                serde_json::from_reader(reader).map_err(trpc::Error::serde)?;
            wrapper.into_result()
        }
    }
}
