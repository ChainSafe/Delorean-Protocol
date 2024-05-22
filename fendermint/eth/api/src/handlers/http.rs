// Copyright 2022-2024 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Based on https://github.com/ChainSafe/forest/blob/v0.8.2/node/rpc/src/rpc_http_handler.rs

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use jsonrpc_v2::{RequestObject, ResponseObjects};
use serde::Deserialize;

use crate::{apis, AppState};

type ResponseHeaders = [(&'static str, &'static str); 1];

const RESPONSE_HEADERS: ResponseHeaders = [("content-type", "application/json-rpc;charset=utf-8")];

/// The Ethereum API implementations accept `{}` or `[{}, {}, ...]` as requests,
/// with the expectation of as many responses.
///
/// `jsonrpc_v2` has a type named `RequestKind` but it's not `Deserialize`.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum RequestKind {
    One(RequestObject),
    Many(Vec<RequestObject>),
}

/// Handle JSON-RPC calls.
pub async fn handle(
    _headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(request): axum::Json<RequestKind>,
) -> impl IntoResponse {
    // NOTE: Any authorization can come here.
    let response = match request {
        RequestKind::One(request) => {
            if let Err(response) = check_request(&request) {
                return response;
            }
            state.rpc_server.handle(request).await
        }
        RequestKind::Many(requests) => {
            for request in requests.iter() {
                if let Err(response) = check_request(request) {
                    return response;
                }
            }
            state.rpc_server.handle(requests).await
        }
    };
    debug_response(&response);
    json_response(&response)
}

fn debug_response(response: &ResponseObjects) {
    let debug = |r| {
        tracing::debug!(
            response = serde_json::to_string(r).unwrap_or_else(|e| e.to_string()),
            "RPC response"
        );
    };
    match response {
        ResponseObjects::Empty => {}
        ResponseObjects::One(r) => {
            debug(r);
        }
        ResponseObjects::Many(rs) => {
            for r in rs {
                debug(r);
            }
        }
    }
}

fn json_response(response: &ResponseObjects) -> (StatusCode, ResponseHeaders, std::string::String) {
    match serde_json::to_string(response) {
        Ok(json) => (StatusCode::OK, RESPONSE_HEADERS, json),
        Err(err) => {
            let msg = err.to_string();
            tracing::error!(error = msg, "RPC to JSON failure");
            (StatusCode::INTERNAL_SERVER_ERROR, RESPONSE_HEADERS, msg)
        }
    }
}

fn check_request(
    request: &RequestObject,
) -> Result<(), (StatusCode, ResponseHeaders, std::string::String)> {
    tracing::debug!(?request, "RPC request");
    let method = request.method_ref().to_owned();

    if apis::is_streaming_method(&method) {
        Err((
            StatusCode::BAD_REQUEST,
            RESPONSE_HEADERS,
            format!("'{method}' is only available through WebSocket"),
        ))
    } else {
        Ok(())
    }
}
