// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{pin::Pin, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use fendermint_rpc::client::{http_client, ws_client};
use futures::Future;
use tendermint_rpc::{
    error::ErrorDetail, query::Query, Client, Error, HttpClient, SimpleRequest, Subscription,
    SubscriptionClient, Url, WebSocketClient, WebSocketClientDriver, WebSocketClientUrl,
};

/// A mixed HTTP and WebSocket client. Uses HTTP to perform all
/// the JSON-RPC requests except the ones which require subscription,
/// which go through a WebSocket client.
///
/// The WebSocket client is expected to lose connection with CometBFT,
/// in which case it will be re-established in the background.
///
/// Existing subscriptions should receive an error and they can try
/// re-subscribing through the Ethereum API facade, which should create
/// new subscriptions through a fresh CometBFT client.
#[derive(Clone)]
pub struct HybridClient {
    http_client: HttpClient,
    cmd_tx: tokio::sync::mpsc::UnboundedSender<DriverCommand>,
}

pub struct HybridClientDriver {
    ws_url: WebSocketClientUrl,
    retry_delay: Duration,
    cmd_rx: tokio::sync::mpsc::UnboundedReceiver<DriverCommand>,
}

enum DriverCommand {
    Subscribe(
        Query,
        tokio::sync::oneshot::Sender<Result<Subscription, Error>>,
    ),
    Unsubscribe(Query, tokio::sync::oneshot::Sender<Result<(), Error>>),
    Close,
}

impl HybridClient {
    pub fn new(
        http_url: Url,
        ws_url: WebSocketClientUrl,
        retry_delay: Duration,
    ) -> anyhow::Result<(Self, HybridClientDriver)> {
        let http_client =
            http_client(http_url, None).context("failed to create Tendermint client")?;

        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();

        let client = Self {
            http_client,
            cmd_tx,
        };

        let driver = HybridClientDriver {
            ws_url,
            retry_delay,
            cmd_rx,
        };

        Ok((client, driver))
    }
}

#[async_trait]
impl Client for HybridClient {
    async fn perform<R>(&self, request: R) -> Result<R::Output, Error>
    where
        R: SimpleRequest,
    {
        self.http_client.perform(request).await
    }
}

#[async_trait]
impl SubscriptionClient for HybridClient {
    async fn subscribe(&self, query: Query) -> Result<Subscription, Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.cmd_tx
            .send(DriverCommand::Subscribe(query, tx))
            .map_err(|_| Error::channel_send())?;

        rx.await
            .map_err(|e| Error::client_internal(e.to_string()))?
    }

    async fn unsubscribe(&self, query: Query) -> Result<(), Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.cmd_tx
            .send(DriverCommand::Unsubscribe(query, tx))
            .map_err(|_| Error::channel_send())?;

        rx.await
            .map_err(|e| Error::client_internal(e.to_string()))?
    }

    fn close(self) -> Result<(), Error> {
        self.cmd_tx
            .send(DriverCommand::Close)
            .map_err(|_| Error::channel_send())
    }
}

impl HybridClientDriver {
    pub async fn run(mut self) {
        let mut client = self.ws_client().await;

        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                DriverCommand::Subscribe(query, tx) => {
                    client = self
                        .send_loop(client, tx, |client| {
                            let query = query.clone();
                            Box::pin(async move { client.subscribe(query.clone()).await })
                        })
                        .await;
                }
                DriverCommand::Unsubscribe(query, tx) => {
                    client = self
                        .send_loop(client, tx, |client| {
                            let query = query.clone();
                            Box::pin(async move { client.unsubscribe(query.clone()).await })
                        })
                        .await;
                }
                DriverCommand::Close => {
                    break;
                }
            }
        }
        let _ = client.close();
    }

    /// Try to send something to the socket. If it fails, reconnect and send again.
    async fn send_loop<F, T>(
        &self,
        mut client: WebSocketClient,
        tx: tokio::sync::oneshot::Sender<Result<T, Error>>,
        f: F,
    ) -> WebSocketClient
    where
        F: Fn(WebSocketClient) -> Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>,
    {
        loop {
            match f(client.clone()).await {
                Err(e) if matches!(e.detail(), ErrorDetail::ChannelSend(_)) => {
                    client = self.ws_client().await;
                }
                res => {
                    let _ = tx.send(res);
                    return client;
                }
            }
        }
    }

    /// Connect to the WebSocket and start the driver, returning the client.
    async fn ws_client(&self) -> WebSocketClient {
        let (client, driver) = self.ws_connect().await;
        tokio::spawn(async move { driver.run().await });
        client
    }

    /// Try connecting repeatedly until it succeeds.
    async fn ws_connect(&self) -> (WebSocketClient, WebSocketClientDriver) {
        let url: Url = self.ws_url.clone().into();
        loop {
            match ws_client(url.clone()).await {
                Ok(cd) => {
                    return cd;
                }
                Err(e) => {
                    tracing::warn!(
                        error = e.to_string(),
                        url = url.to_string(),
                        "failed to connect to Tendermint WebSocket; retrying in {}s...",
                        self.retry_delay.as_secs()
                    );
                    tokio::time::sleep(self.retry_delay).await;
                }
            }
        }
    }
}
