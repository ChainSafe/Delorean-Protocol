// Copyright 2022-2024 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Based on https://github.com/ChainSafe/forest/blob/v0.8.2/node/rpc/src/rpc_ws_handler.rs

use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    http::HeaderMap,
    response::IntoResponse,
};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use jsonrpc_v2::{RequestObject, ResponseObject, ResponseObjects, V2};
use serde_json::json;

use crate::{apis, state::WebSocketId, AppState, JsonRpcServer};

/// Mirroring [ethers_providers::rpc::transports::ws::types::Notification], which is what the library
/// expects for non-request-response payloads in [PubSubItem::deserialize].
#[derive(Debug)]
pub struct Notification {
    pub subscription: ethers_core::types::U256,
    pub result: serde_json::Value,
}

#[derive(Debug)]
pub struct MethodNotification {
    // There is only one streaming method at the moment, but let's not hardcode it here.
    pub method: String,
    pub notification: Notification,
}

pub async fn handle(
    _headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async { rpc_ws_handler_inner(state, socket).await })
}

/// Handle requests in a loop, interpreting each message as a JSON-RPC request.
///
/// Messages are evaluated one by one. We could spawn tasks like Forest,
/// but there should be some rate limiting applied to avoid DoS attacks.
async fn rpc_ws_handler_inner(state: AppState, socket: WebSocket) {
    tracing::debug!("Accepted WS connection!");
    let (mut sender, mut receiver) = socket.split();

    // Create a channel over which the application can send messages to this socket.
    let (notif_tx, mut notif_rx) = tokio::sync::mpsc::unbounded_channel();

    let web_socket_id = state.rpc_state.add_web_socket(notif_tx).await;

    loop {
        let keep = tokio::select! {
            Some(Ok(message)) = receiver.next() => {
                handle_incoming(web_socket_id, &state.rpc_server, &mut sender, message).await
            },
            Some(notif) = notif_rx.recv() => {
                handle_outgoing(web_socket_id, &mut sender, notif).await
            },
            else => break,
        };

        if !keep {
            break;
        }
    }

    // Clean up.
    tracing::debug!(web_socket_id, "Removing WS connection");
    state.rpc_state.remove_web_socket(&web_socket_id).await;
}

/// Handle an incoming request.
async fn handle_incoming(
    web_socket_id: WebSocketId,
    rpc_server: &JsonRpcServer,
    sender: &mut SplitSink<WebSocket, Message>,
    message: Message,
) -> bool {
    if let Message::Text(mut request_text) = message {
        if !request_text.is_empty() {
            tracing::debug!(web_socket_id, request = request_text, "WS Request Received");

            // We have to deserialize-add-reserialize becuase `JsonRpcRequest` can
            // only be parsed with `from_str`, not `from_value`.
            request_text = maybe_add_web_socket_id(request_text, web_socket_id);

            match serde_json::from_str::<RequestObject>(&request_text) {
                Ok(req) => {
                    return send_call_result(web_socket_id, rpc_server, sender, req).await;
                }
                Err(e) => {
                    deserialization_error("RequestObject", e);
                }
            }
        }
    }
    true
}

fn deserialization_error(what: &str, e: serde_json::Error) {
    // Not responding to the websocket because it requires valid responses, which need to have
    // the `id` field present, which we'd only get if we managed to parse the request.
    // Using `debug!` so someone sending junk cannot flood the log with warnings.
    tracing::debug!("Error deserializing WS payload as {what}: {e}");
}

/// Try to append the websocket ID to the parameters if the method is a streaming one.
///
/// This is best effort. If fails, just let the JSON-RPC server handle the problem.
fn maybe_add_web_socket_id(request_text: String, web_socket_id: WebSocketId) -> String {
    match serde_json::from_str::<serde_json::Value>(&request_text) {
        Ok(mut json) => {
            // If the method requires web sockets, append the ID of the socket to the parameters.
            let is_streaming = match json.get("method") {
                Some(serde_json::Value::String(method)) => apis::is_streaming_method(method),
                _ => false,
            };

            if is_streaming {
                match json.get_mut("params") {
                    Some(serde_json::Value::Array(ref mut params)) => {
                        params.push(serde_json::Value::Number(serde_json::Number::from(
                            web_socket_id,
                        )));

                        return serde_json::to_string(&json).unwrap_or(request_text);
                    }
                    _ => {
                        tracing::debug!("JSON-RPC streaming request has no or unexpected params")
                    }
                }
            }
        }
        Err(e) => {
            deserialization_error("JSON", e);
        }
    }
    request_text
}

/// Send a message from the application, result of an async subscription.
///
/// Returns `false` if the socket has been closed, otherwise `true` to keep working.
async fn handle_outgoing(
    web_socket_id: WebSocketId,
    sender: &mut SplitSink<WebSocket, Message>,
    notif: MethodNotification,
) -> bool {
    // Based on https://github.com/gakonst/ethers-rs/blob/ethers-v2.0.7/ethers-providers/src/rpc/transports/ws/types.rs#L145
    let message = json! ({
        "jsonrpc": V2,
        "method": notif.method,
        "params": {
            "subscription": notif.notification.subscription,
            "result": notif.notification.result
        }
    });

    match serde_json::to_string(&message) {
        Err(e) => {
            tracing::error!(error=?e, "failed to serialize notification to JSON");
        }
        Ok(json) => {
            tracing::debug!(web_socket_id, json, "sending notification to WS");
            if let Err(e) = sender.send(Message::Text(json)).await {
                tracing::warn!(web_socket_id, error =? e, "failed to send notfication to WS");
                if is_closed_connection(e) {
                    return false;
                }
            }
        }
    }
    true
}

/// Call the RPC method and respond through the Web Socket.
async fn send_call_result(
    web_socket_id: WebSocketId,
    server: &JsonRpcServer,
    sender: &mut SplitSink<WebSocket, Message>,
    request: RequestObject,
) -> bool {
    let method = request.method_ref();

    tracing::debug!("RPC WS called method: {}", method);

    match server.handle(request).await {
        ResponseObjects::Empty => true,
        ResponseObjects::One(response) => send_response(web_socket_id, sender, response).await,
        ResponseObjects::Many(responses) => {
            for response in responses {
                if !send_response(web_socket_id, sender, response).await {
                    return false;
                }
            }
            true
        }
    }
}

async fn send_response(
    web_socket_id: WebSocketId,
    sender: &mut SplitSink<WebSocket, Message>,
    response: ResponseObject,
) -> bool {
    let response = serde_json::to_string(&response);

    match response {
        Err(e) => {
            tracing::error!(error=?e, "failed to serialize response to JSON");
        }
        Ok(json) => {
            tracing::debug!(web_socket_id, json, "sending response to WS");
            if let Err(e) = sender.send(Message::Text(json)).await {
                tracing::warn!(web_socket_id, error=?e, "failed to send response to WS");
                if is_closed_connection(e) {
                    return false;
                }
            }
        }
    }
    true
}

fn is_closed_connection(e: axum::Error) -> bool {
    e.to_string().contains("closed connection")
}

#[cfg(test)]
mod tests {

    #[test]
    fn can_parse_request() {
        let text = "{\"id\":0,\"jsonrpc\":\"2.0\",\"method\":\"eth_newFilter\",\"params\":[{\"topics\":[]}]}";
        let _value = serde_json::from_str::<serde_json::Value>(text).expect("should parse as JSON");
        // The following would fail because `V2` expects an `&str` but the `from_value` deserialized returns `String`.
        // let _request = serde_json::from_value::<jsonrpc_v2::RequestObject>(value)
        //     .expect("should parse as JSON-RPC request");
    }
}
