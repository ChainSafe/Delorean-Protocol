// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use cid::multihash::MultihashDigest;
use jsonrpc_v2::Params;
use tendermint::abci;
use tendermint_rpc::Client;

use crate::{JsonRpcData, JsonRpcResult};

/// Returns the current client version.
pub async fn client_version<C>(data: JsonRpcData<C>) -> JsonRpcResult<String>
where
    C: Client + Sync + Send,
{
    let res: abci::response::Info = data
        .tm()
        .abci_info()
        .await
        .context("failed to fetch info")?;

    let version = format!("{}/{}/{}", res.data, res.version, res.app_version);

    Ok(version)
}

/// Returns Keccak-256 (not the standardized SHA3-256) of the given data.
///
/// Expects the data as hex encoded string and returns it as such.
pub async fn sha3<C>(
    _data: JsonRpcData<C>,
    Params((input,)): Params<(String,)>,
) -> JsonRpcResult<String>
where
    C: Client + Sync + Send,
{
    let input = input.strip_prefix("0x").unwrap_or(&input);
    let input = hex::decode(input).context("failed to decode input as hex")?;
    let output = cid::multihash::Code::Keccak256.digest(&input);
    let output = hex::encode(output.digest());
    Ok(output)
}
