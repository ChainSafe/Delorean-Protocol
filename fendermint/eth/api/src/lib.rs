// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use axum::routing::{get, post};
use fvm_shared::econ::TokenAmount;
use jsonrpc_v2::Data;
use std::{net::ToSocketAddrs, sync::Arc, time::Duration};

mod apis;
mod cache;
mod client;
mod conv;
mod error;
mod filters;
mod gas;
mod handlers;
mod mpool;
mod state;

pub use client::{HybridClient, HybridClientDriver};

use error::{error, JsonRpcError};
use state::{JsonRpcState, Nonce};

/// This is passed to every method handler. It's generic in the client type to facilitate testing with mocks.
type JsonRpcData<C> = Data<JsonRpcState<C>>;
type JsonRpcServer = Arc<jsonrpc_v2::Server<jsonrpc_v2::MapRouter>>;
type JsonRpcResult<T> = Result<T, JsonRpcError>;

/// This is the state we will pass to [axum] so that we can extract it in handlers.
#[derive(Clone)]
pub struct AppState {
    pub rpc_server: JsonRpcServer,
    pub rpc_state: Arc<JsonRpcState<HybridClient>>,
}

#[derive(Debug, Clone)]
pub struct GasOpt {
    pub min_gas_premium: TokenAmount,
    pub num_blocks_max_prio_fee: u64,
    pub max_fee_hist_size: u64,
}

/// Start listening to JSON-RPC requests.
pub async fn listen<A: ToSocketAddrs>(
    listen_addr: A,
    client: HybridClient,
    filter_timeout: Duration,
    cache_capacity: usize,
    max_nonce_gap: Nonce,
    gas_opt: GasOpt,
) -> anyhow::Result<()> {
    if let Some(listen_addr) = listen_addr.to_socket_addrs()?.next() {
        let rpc_state = Arc::new(JsonRpcState::new(
            client,
            filter_timeout,
            cache_capacity,
            max_nonce_gap,
            gas_opt,
        ));

        // Start the transaction cache pruning subscription.
        mpool::start_tx_cache_clearing(
            rpc_state.client.clone(),
            rpc_state.tx_cache.clone(),
            rpc_state.tx_buffer.clone(),
        );

        let rpc_server = make_server(rpc_state.clone());
        let app_state = AppState {
            rpc_server,
            rpc_state,
        };
        let router = make_router(app_state);
        let server = axum::Server::try_bind(&listen_addr)?.serve(router.into_make_service());
        tracing::info!(?listen_addr, "bound Ethereum API");
        server.await?;
        Ok(())
    } else {
        Err(anyhow!("failed to convert to any socket address"))
    }
}

/// Register method handlers with the JSON-RPC server construct.
fn make_server(state: Arc<JsonRpcState<HybridClient>>) -> JsonRpcServer {
    let server = jsonrpc_v2::Server::new().with_data(Data(state));
    let server = apis::register_methods(server);
    server.finish()
}

/// Register routes in the `axum` HTTP router to handle JSON-RPC and WebSocket calls.
fn make_router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/", post(handlers::http::handle))
        .route("/", get(handlers::ws::handle))
        .with_state(state)
}
