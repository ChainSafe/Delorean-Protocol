// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use futures_util::StreamExt;
use serde_json::json;
use url::Url;

use crate::jsonrpc::{JsonRpcClient, JsonRpcClientImpl, NO_PARAMS};

/// The default endpoints for public lotus node. If the urls fail in running tests, need to
/// check these endpoints again.
const HTTP_ENDPOINT: &str = "https://api.node.glif.io/rpc/v0";
const WS_ENDPOINT: &str = "wss://wss.node.glif.io/apigw/lotus/rpc/v0";

#[tokio::test]
async fn test_request_error() {
    let url = Url::parse(HTTP_ENDPOINT).unwrap();
    let client = JsonRpcClientImpl::new(url, None);
    // Make a request with missing params
    let response = client
        .request::<serde_json::Value>("Filecoin.ChainGetBlock", NO_PARAMS)
        .await;
    assert!(response.is_err());
}

#[tokio::test]
#[ignore]
async fn test_request_with_params() {
    let url = Url::parse(HTTP_ENDPOINT).unwrap();
    let client = JsonRpcClientImpl::new(url, None);

    let params = json!([{"/":"bafy2bzacecwgnejfzcq7a4zvvownmb4oae6xzyu323z5wuuufesbtikortt6k"}]);
    let response = client
        .request::<serde_json::Value>("Filecoin.ChainGetBlock", params)
        .await;
    assert!(response.is_ok());
}

#[tokio::test]
async fn test_request_with_params_error() {
    let url = Url::parse(HTTP_ENDPOINT).unwrap();
    let client = JsonRpcClientImpl::new(url, None);

    let response = client
        .request::<serde_json::Value>("Filecoin.ChainGetBlock", NO_PARAMS)
        .await;
    assert!(response.is_err());
}

#[tokio::test]
#[ignore]
async fn test_subscribe() {
    let url = Url::parse(WS_ENDPOINT).unwrap();
    let client = JsonRpcClientImpl::new(url, None);
    let mut chan = client.subscribe("Filecoin.ChainNotify").await.unwrap();
    for _ in 1..=3 {
        chan.next().await.unwrap();
    }
}
