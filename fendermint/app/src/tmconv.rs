// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Conversions to Tendermint data types.
use anyhow::{anyhow, bail, Context};
use fendermint_vm_core::Timestamp;
use fendermint_vm_genesis::{Power, Validator};
use fendermint_vm_interpreter::fvm::{
    state::{BlockHash, FvmStateParams},
    FvmApplyRet, FvmCheckRet, FvmQueryRet, PowerUpdates,
};
use fendermint_vm_message::signed::DomainHash;
use fendermint_vm_snapshot::{SnapshotItem, SnapshotManifest};
use fvm_shared::{address::Address, error::ExitCode, event::StampedEvent, ActorID};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, num::NonZeroU32};
use tendermint::abci::{response, Code, Event, EventAttribute};

use crate::{app::AppError, BlockHeight};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SnapshotMetadata {
    size: u64,
    state_params: FvmStateParams,
}

/// IPLD encoding of data types we know we must be able to encode.
macro_rules! ipld_encode {
    ($var:ident) => {
        fvm_ipld_encoding::to_vec(&$var)
            .map_err(|e| anyhow!("error IPLD encoding {}: {}", stringify!($var), e))?
    };
}

/// Response to delivery where the input was blatantly invalid.
/// This indicates that the validator who made the block was Byzantine.
pub fn invalid_deliver_tx(err: AppError, description: String) -> response::DeliverTx {
    tracing::info!(error = ?err, description, "invalid deliver_tx");
    response::DeliverTx {
        code: Code::Err(NonZeroU32::try_from(err as u32).expect("error codes are non-zero")),
        info: description,
        ..Default::default()
    }
}

/// Response to checks where the input was blatantly invalid.
/// This indicates that the user who sent the transaction is either attacking or has a faulty client.
pub fn invalid_check_tx(err: AppError, description: String) -> response::CheckTx {
    tracing::info!(error = ?err, description, "invalid check_tx");
    response::CheckTx {
        code: Code::Err(NonZeroU32::try_from(err as u32).expect("error codes are non-zero")),
        info: description,
        ..Default::default()
    }
}

/// Response to queries where the input was blatantly invalid.
pub fn invalid_query(err: AppError, description: String) -> response::Query {
    tracing::info!(error = ?err, description, "invalid query");
    response::Query {
        code: Code::Err(NonZeroU32::try_from(err as u32).expect("error codes are non-zero")),
        info: description,
        ..Default::default()
    }
}

pub fn to_deliver_tx(
    ret: FvmApplyRet,
    domain_hash: Option<DomainHash>,
    block_hash: Option<BlockHash>,
) -> response::DeliverTx {
    let receipt = ret.apply_ret.msg_receipt;

    // Based on the sanity check in the `DefaultExecutor`.
    // gas_cost = gas_fee_cap * gas_limit; this is how much the account is charged up front.
    // &base_fee_burn + &over_estimation_burn + &refund + &miner_tip == gas_cost
    // But that's in tokens. I guess the closes to what we want is the limit.
    let gas_wanted: i64 = ret.gas_limit.try_into().unwrap_or(i64::MAX);
    let gas_used: i64 = receipt.gas_used.try_into().unwrap_or(i64::MAX);

    // This should return the `RawBytes` as-is, which is IPLD encoded content.
    let data: bytes::Bytes = receipt.return_data.to_vec().into();
    let mut events = to_events("event", ret.apply_ret.events, ret.emitters);

    // Emit the block hash. It's not useful to subscribe by as it's a-priori unknown,
    // but we can use it during subscription to fill in the block hash field which Ethereum
    // subscriptions expect, and it's otherwise not available.
    if let Some(h) = block_hash {
        events.push(Event::new(
            "block",
            vec![EventAttribute {
                key: "hash".to_string(),
                value: hex::encode(h),
                index: true,
            }],
        ));
    }

    // Emit an event which causes Tendermint to index our transaction with a custom hash.
    // In theory we could emit multiple values under `tx.hash`, but in subscriptions we are
    // looking to emit the one expected by Ethereum clients.
    if let Some(h) = domain_hash {
        events.push(to_domain_hash_event(&h));
    }

    // Emit general message metadata.
    events.push(to_message_event(ret.from, ret.to));

    let message = ret
        .apply_ret
        .failure_info
        .map(|i| i.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| to_error_msg(receipt.exit_code).to_owned());

    response::DeliverTx {
        code: to_code(receipt.exit_code),
        data,
        log: Default::default(),
        info: message,
        gas_wanted,
        gas_used,
        events,
        codespace: Default::default(),
    }
}

pub fn to_check_tx(ret: FvmCheckRet) -> response::CheckTx {
    // Putting the message `log` because only `log` appears in the `tx_sync` JSON-RPC response.
    let message = ret
        .info
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| to_error_msg(ret.exit_code).to_owned());

    // Potential error messages that arise in checking if contract execution is enabled are returned in the data.
    // See https://github.com/gakonst/ethers-rs/commit/860100535812cbfe5e3cc417872392a6d76a159c for examples.
    // Do this the same way as `to_deliver_tx`, serializing to IPLD.
    let data: bytes::Bytes = ret.return_data.unwrap_or_default().to_vec().into();

    response::CheckTx {
        code: to_code(ret.exit_code),
        log: message.clone(),
        info: Default::default(),
        data,
        gas_wanted: ret.gas_limit.try_into().unwrap_or(i64::MAX),
        sender: ret.sender.to_string(),
        ..Default::default()
    }
}

/// Map the return values from epoch boundary operations to validator updates.
pub fn to_end_block(power_table: PowerUpdates) -> anyhow::Result<response::EndBlock> {
    let validator_updates =
        to_validator_updates(power_table.0).context("failed to convert validator updates")?;

    Ok(response::EndBlock {
        validator_updates,
        consensus_param_updates: None,
        events: Vec::new(), // TODO: Events from epoch transitions?
    })
}

/// Map the return values from cron operations.
pub fn to_begin_block(ret: FvmApplyRet) -> response::BeginBlock {
    let events = to_events("event", ret.apply_ret.events, ret.emitters);

    response::BeginBlock { events }
}

/// Convert events to key-value pairs.
///
/// Fot the EVM, they are returned like so:
///
/// ```text
/// StampedEvent { emitter: 103,
///  event: ActorEvent { entries: [
///    Entry { flags: FLAG_INDEXED_VALUE, key: "t1", value: RawBytes { 5820ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef } },
///    Entry { flags: FLAG_INDEXED_VALUE, key: "t2", value: RawBytes { 54ff00000000000000000000000000000000000065 } },
///    Entry { flags: FLAG_INDEXED_VALUE, key: "t3", value: RawBytes { 54ff00000000000000000000000000000000000066 } },
///    Entry { flags: FLAG_INDEXED_VALUE, key: "d", value: RawBytes { 582000000000000000000000000000000000000000000000000000000000000007d0 } }] } }
/// ```
///
/// The values are:
/// * "t1" will be the cbor encoded keccak-256 hash of the event signature Transfer(address,address,uint256)
/// * "t2" will be the first indexed argument, i.e. _from  (cbor encoded byte array; needs padding to 32 bytes to work with ethers)
/// * "t3" will be the second indexed argument, i.e. _to (cbor encoded byte array; needs padding to 32 bytes to work with ethers)
/// * "d" is a cbor encoded byte array of all the remaining arguments
pub fn to_events(
    kind: &str,
    stamped_events: Vec<StampedEvent>,
    emitters: HashMap<ActorID, Address>,
) -> Vec<Event> {
    stamped_events
        .into_iter()
        .map(|se| {
            let mut attrs = Vec::new();

            attrs.push(EventAttribute {
                key: "emitter.id".to_string(),
                value: se.emitter.to_string(),
                index: true,
            });

            // This is emitted because some clients might want to subscribe to events
            // based on the deterministic Ethereum address even before a contract is created.
            if let Some(deleg_addr) = emitters.get(&se.emitter) {
                attrs.push(EventAttribute {
                    key: "emitter.deleg".to_string(),
                    value: deleg_addr.to_string(),
                    index: true,
                });
            }

            for e in se.event.entries {
                attrs.push(EventAttribute {
                    key: e.key,
                    value: hex::encode(e.value),
                    index: !e.flags.is_empty(),
                });
            }

            Event::new(kind.to_string(), attrs)
        })
        .collect()
}

/// Construct an indexable event from a custom transaction hash.
pub fn to_domain_hash_event(domain_hash: &DomainHash) -> Event {
    let (k, v) = match domain_hash {
        DomainHash::Eth(h) => ("eth", hex::encode(h)),
    };
    Event::new(
        k,
        vec![EventAttribute {
            key: "hash".to_string(),
            value: v,
            index: true,
        }],
    )
}

/// Event about the message itself.
pub fn to_message_event(from: Address, to: Address) -> Event {
    let attr = |k: &str, v: Address| EventAttribute {
        key: k.to_string(),
        value: v.to_string(),
        index: true,
    };
    Event::new(
        "message".to_string(),
        vec![attr("from", from), attr("to", to)],
    )
}

/// Map to query results.
pub fn to_query(ret: FvmQueryRet, block_height: BlockHeight) -> anyhow::Result<response::Query> {
    let exit_code = match ret {
        FvmQueryRet::Ipld(None) | FvmQueryRet::ActorState(None) => ExitCode::USR_NOT_FOUND,
        FvmQueryRet::Ipld(_) | FvmQueryRet::ActorState(_) => ExitCode::OK,
        // For calls and estimates, the caller needs to look into the `value` field to see the real exit code;
        // the query itself is successful, even if the value represents a failure.
        FvmQueryRet::Call(_) | FvmQueryRet::EstimateGas(_) => ExitCode::OK,
        FvmQueryRet::StateParams(_) => ExitCode::OK,
        FvmQueryRet::BuiltinActors(_) => ExitCode::OK,
    };

    // The return value has a `key` field which is supposed to be set to the data matched.
    // Although at this point I don't have access to the input like the CID looked up,
    // but I assume the query sender has. Rather than repeat everything, I'll add the key
    // where it gives some extra information, like the actor ID, just to keep this option visible.
    let (key, value) = match ret {
        FvmQueryRet::Ipld(None) | FvmQueryRet::ActorState(None) => (Vec::new(), Vec::new()),
        FvmQueryRet::Ipld(Some(bz)) => (Vec::new(), bz),
        FvmQueryRet::ActorState(Some(x)) => {
            let (id, st) = *x;
            let k = ipld_encode!(id);
            let v = ipld_encode!(st);
            (k, v)
        }
        FvmQueryRet::Call(ret) => {
            // Send back an entire Tendermint deliver_tx response, encoded as IPLD.
            // This is so there is a single representation of a call result, instead
            // of a normal delivery being one way and a query exposing `FvmApplyRet`.
            let dtx = to_deliver_tx(ret, None, None);
            let dtx = tendermint_proto::abci::ResponseDeliverTx::from(dtx);
            let mut buf = bytes::BytesMut::new();
            dtx.encode(&mut buf)?;
            let bz = buf.to_vec();
            // So the value is an IPLD encoded Protobuf byte vector.
            let v = ipld_encode!(bz);
            (Vec::new(), v)
        }
        FvmQueryRet::EstimateGas(est) => {
            let v = ipld_encode!(est);
            (Vec::new(), v)
        }
        FvmQueryRet::StateParams(sp) => {
            let v = ipld_encode!(sp);
            (Vec::new(), v)
        }
        FvmQueryRet::BuiltinActors(ba) => {
            let v = ipld_encode!(ba);
            (Vec::new(), v)
        }
    };

    // The height here is the height of the block that was committed, not in which the app hash appeared.
    let height = tendermint::block::Height::try_from(block_height).context("height too big")?;

    let res = response::Query {
        code: to_code(exit_code),
        info: to_error_msg(exit_code).to_owned(),
        key: key.into(),
        value: value.into(),
        height,
        ..Default::default()
    };

    Ok(res)
}

/// Project Genesis validators to Tendermint.
pub fn to_validator_updates(
    validators: Vec<Validator<Power>>,
) -> anyhow::Result<Vec<tendermint::validator::Update>> {
    let mut updates = vec![];
    for v in validators {
        updates.push(tendermint::validator::Update {
            pub_key: tendermint::PublicKey::try_from(v.public_key)?,
            power: tendermint::vote::Power::try_from(v.power.0)?,
        });
    }
    Ok(updates)
}

pub fn to_timestamp(time: tendermint::time::Time) -> Timestamp {
    Timestamp(
        time.unix_timestamp()
            .try_into()
            .expect("negative timestamp"),
    )
}

pub fn to_code(exit_code: ExitCode) -> Code {
    if exit_code.is_success() {
        Code::Ok
    } else {
        Code::Err(NonZeroU32::try_from(exit_code.value()).expect("error codes are non-zero"))
    }
}

pub fn to_error_msg(exit_code: ExitCode) -> &'static str {
    match exit_code {
        ExitCode::OK => "",
        ExitCode::SYS_SENDER_INVALID => "The message sender doesn't exist.",
        ExitCode::SYS_SENDER_STATE_INVALID => "The message sender was not in a valid state to send this message. Either the nonce didn't match, or the sender didn't have funds to cover the message gas.",
        ExitCode::SYS_ILLEGAL_INSTRUCTION => "The message receiver trapped (panicked).",
        ExitCode::SYS_INVALID_RECEIVER => "The message receiver doesn't exist and can't be automatically created",
        ExitCode::SYS_INSUFFICIENT_FUNDS => "The message sender didn't have the requisite funds.",
        ExitCode::SYS_OUT_OF_GAS => "Message execution (including subcalls) used more gas than the specified limit.",
        ExitCode::SYS_ILLEGAL_EXIT_CODE => "The message receiver aborted with a reserved exit code.",
        ExitCode::SYS_ASSERTION_FAILED => "An internal VM assertion failed.",
        ExitCode::SYS_MISSING_RETURN => "The actor returned a block handle that doesn't exist",
        ExitCode::USR_ILLEGAL_ARGUMENT => "The method parameters are invalid.",
        ExitCode::USR_NOT_FOUND => "The requested resource does not exist.",
        ExitCode::USR_FORBIDDEN => "The requested operation is forbidden.",
        ExitCode::USR_INSUFFICIENT_FUNDS => "The actor has insufficient funds to perform the requested operation.",
        ExitCode::USR_ILLEGAL_STATE => "The actor's internal state is invalid.",
        ExitCode::USR_SERIALIZATION => "There was a de/serialization failure within actor code.",
        ExitCode::USR_UNHANDLED_MESSAGE => "The message cannot be handled (usually indicates an unhandled method number).",
        ExitCode::USR_UNSPECIFIED => "The actor failed with an unspecified error.",
        ExitCode::USR_ASSERTION_FAILED => "The actor failed a user-level assertion.",
        ExitCode::USR_READ_ONLY => "The requested operation cannot be performed in 'read-only' mode.",
        ExitCode::USR_NOT_PAYABLE => "The method cannot handle a transfer of value.",
        _ => ""
    }
}

pub fn to_snapshots(
    snapshots: impl IntoIterator<Item = SnapshotItem>,
) -> anyhow::Result<response::ListSnapshots> {
    let snapshots = snapshots
        .into_iter()
        .map(to_snapshot)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(response::ListSnapshots { snapshots })
}

/// Convert a snapshot manifest to the Tendermint ABCI type.
pub fn to_snapshot(snapshot: SnapshotItem) -> anyhow::Result<tendermint::abci::types::Snapshot> {
    // Put anything that doesn't fit into fields of the ABCI snapshot into the metadata.
    let metadata = SnapshotMetadata {
        size: snapshot.manifest.size,
        state_params: snapshot.manifest.state_params,
    };

    Ok(tendermint::abci::types::Snapshot {
        height: snapshot
            .manifest
            .block_height
            .try_into()
            .expect("height is valid"),
        format: snapshot.manifest.version,
        chunks: snapshot.manifest.chunks,
        hash: snapshot.manifest.checksum.into(),
        metadata: fvm_ipld_encoding::to_vec(&metadata)?.into(),
    })
}

/// Parse a Tendermint ABCI snapshot offer to a manifest.
pub fn from_snapshot(
    offer: tendermint::abci::request::OfferSnapshot,
) -> anyhow::Result<SnapshotManifest> {
    let metadata = fvm_ipld_encoding::from_slice::<SnapshotMetadata>(&offer.snapshot.metadata)
        .context("failed to parse snapshot metadata")?;

    let app_hash = to_app_hash(&metadata.state_params);

    if app_hash != offer.app_hash {
        bail!(
            "the application hash does not match the metadata; from-meta = {}, from-offer = {}",
            app_hash,
            offer.app_hash,
        );
    }

    let checksum = tendermint::hash::Hash::try_from(offer.snapshot.hash)
        .context("failed to parse checksum")?;

    let manifest = SnapshotManifest {
        block_height: offer.snapshot.height.value(),
        size: metadata.size,
        chunks: offer.snapshot.chunks,
        checksum,
        state_params: metadata.state_params,
        version: offer.snapshot.format,
    };

    Ok(manifest)
}

/// Produce an appliction hash that is a commitment to all data replicated by consensus,
/// that is, all nodes participating in the network must agree on this otherwise we have
/// a consensus failure.
///
/// Notably it contains the actor state root _as well as_ some of the metadata maintained
/// outside the FVM, such as the timestamp and the circulating supply.
pub fn to_app_hash(state_params: &FvmStateParams) -> tendermint::hash::AppHash {
    // Create an artifical CID from the FVM state params, which include everything that
    // deterministically changes under consensus.
    let state_params_cid =
        fendermint_vm_message::cid(state_params).expect("state params have a CID");

    // We could reduce it to a hash to ephasize that this is not something that we can return at the moment,
    // but we could just as easily store the record in the Blockstore to make it retrievable.
    // It is generally not a goal to serve the entire state over the IPLD Resolver or ABCI queries, though;
    // for that we should rely on the CometBFT snapshot mechanism.
    // But to keep our options open, we can return the hash as a CID that nobody can retrieve, and change our mind later.

    // let state_params_hash = state_params_cid.hash();
    let state_params_hash = state_params_cid.to_bytes();

    tendermint::hash::AppHash::try_from(state_params_hash).expect("hash can be wrapped")
}

#[cfg(test)]
mod tests {
    use fendermint_vm_snapshot::SnapshotItem;
    use fvm_shared::error::ExitCode;
    use tendermint::abci::request;

    use crate::tmconv::to_error_msg;

    use super::{from_snapshot, to_app_hash, to_snapshot};

    #[test]
    fn code_error_message() {
        assert_eq!(to_error_msg(ExitCode::OK), "");
        assert_eq!(
            to_error_msg(ExitCode::SYS_SENDER_INVALID),
            "The message sender doesn't exist."
        );
    }

    #[quickcheck_macros::quickcheck]
    fn abci_snapshot_metadata(snapshot: SnapshotItem) {
        let abci_snapshot = to_snapshot(snapshot.clone()).unwrap();
        let abci_offer = request::OfferSnapshot {
            snapshot: abci_snapshot,
            app_hash: to_app_hash(&snapshot.manifest.state_params),
        };
        let manifest = from_snapshot(abci_offer).unwrap();
        assert_eq!(manifest, snapshot.manifest)
    }
}
