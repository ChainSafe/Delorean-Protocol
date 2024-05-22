// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::{anyhow, Context};
use base64::Engine;
use bytes::Bytes;
use fendermint_vm_actor_interface::eam::{self, CreateReturn};
use fvm_ipld_encoding::{BytesDe, RawBytes};
use tendermint::abci::response::DeliverTx;

/// Parse what Tendermint returns in the `data` field of [`DeliverTx`] into bytes.
/// Somewhere along the way it replaces them with the bytes of a Base64 encoded string,
/// and `tendermint_rpc` does not undo that wrapping.
pub fn decode_data(data: &Bytes) -> anyhow::Result<RawBytes> {
    let b64 = String::from_utf8(data.to_vec()).context("error parsing data as base64 string")?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .context("error parsing base64 to bytes")?;
    Ok(RawBytes::from(data))
}

/// Apply the encoding that Tendermint does to the bytes inside [`DeliverTx`].
pub fn encode_data(data: &[u8]) -> Bytes {
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    let bz = b64.as_bytes();
    Bytes::copy_from_slice(bz)
}

/// Parse what Tendermint returns in the `data` field of [`DeliverTx`] as raw bytes.
///
/// Only call this after the `code` of both [`DeliverTx`] and [`CheckTx`] have been inspected!
pub fn decode_bytes(deliver_tx: &DeliverTx) -> anyhow::Result<RawBytes> {
    decode_data(&deliver_tx.data)
}

/// Parse what Tendermint returns in the `data` field of [`DeliverTx`] as [`CreateReturn`].
pub fn decode_fevm_create(deliver_tx: &DeliverTx) -> anyhow::Result<CreateReturn> {
    let data = decode_data(&deliver_tx.data)?;
    fvm_ipld_encoding::from_slice::<eam::CreateReturn>(&data)
        .map_err(|e| anyhow!("error parsing as CreateReturn: {e}"))
}

/// Parse what Tendermint returns in the `data` field of [`DeliverTx`] as raw ABI return value.
pub fn decode_fevm_invoke(deliver_tx: &DeliverTx) -> anyhow::Result<Vec<u8>> {
    let data = decode_data(&deliver_tx.data)?;
    decode_fevm_return_data(data)
}

/// Parse what is in the `return_data` field, which is `RawBytes` containing IPLD encoded bytes, into the really raw content.
pub fn decode_fevm_return_data(data: RawBytes) -> anyhow::Result<Vec<u8>> {
    // Some calls like transfers between Ethereum accounts don't return any data.
    if data.is_empty() {
        return Ok(data.into());
    }

    // This is the data return by the FEVM itself, not something wrapping another piece,
    // that is, it's as if it was returning `CreateReturn`, it's returning `RawBytes` encoded as IPLD.
    fvm_ipld_encoding::from_slice::<BytesDe>(&data)
        .map(|bz| bz.0)
        .map_err(|e| anyhow!("failed to deserialize bytes returned by FEVM method invocation: {e}"))
}
