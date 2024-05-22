// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use ethers_core::types as et;
use fendermint_rpc::query::QueryClient;
use fendermint_vm_message::query::FvmQueryHeight;
use tendermint_rpc::endpoint::net_info;
use tendermint_rpc::Client;

use crate::{JsonRpcData, JsonRpcResult};

/// The current FVM network version.
///
/// Same as eth_protocolVersion
pub async fn version<C>(data: JsonRpcData<C>) -> JsonRpcResult<String>
where
    C: Client + Sync + Send,
{
    let res = data.client.state_params(FvmQueryHeight::default()).await?;
    let version: u32 = res.value.network_version.into();
    Ok(version.to_string())
}

/// Returns true if client is actively listening for network connections.
pub async fn listening<C>(data: JsonRpcData<C>) -> JsonRpcResult<bool>
where
    C: Client + Sync + Send,
{
    let res: net_info::Response = data
        .tm()
        .net_info()
        .await
        .context("failed to fetch net_info")?;

    Ok(res.listening)
}

/// Returns true if client is actively listening for network connections.
pub async fn peer_count<C>(data: JsonRpcData<C>) -> JsonRpcResult<et::U64>
where
    C: Client + Sync + Send,
{
    let res: net_info::Response = data
        .tm()
        .net_info()
        .await
        .context("failed to fetch net_info")?;

    Ok(et::U64::from(res.n_peers))
}
