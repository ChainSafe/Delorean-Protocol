// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helper methods to convert between Ethereum and Tendermint data formats.

use std::collections::HashSet;
use std::str::FromStr;

use anyhow::{anyhow, Context};
use ethers_core::types::{self as et};
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_message::conv::from_fvm::to_eth_transaction_request;
use fendermint_vm_message::{chain::ChainMessage, signed::SignedMessage};
use fvm_shared::address::Address;
use fvm_shared::bigint::Zero;
use fvm_shared::chainid::ChainID;
use fvm_shared::{bigint::BigInt, econ::TokenAmount};
use lazy_static::lazy_static;
use tendermint::abci::types::ExecTxResult;
use tendermint::abci::{self, Event, EventAttribute};
use tendermint::crypto::sha256::Sha256;
use tendermint_rpc::endpoint;

use super::from_eth;
use super::from_fvm::{to_eth_address, to_eth_signature, to_eth_tokens};

// Values taken from https://github.com/filecoin-project/lotus/blob/6e7dc9532abdb3171427347710df4c860f1957a2/chain/types/ethtypes/eth_types.go#L199

lazy_static! {
    static ref EMPTY_ETH_HASH: et::H256 = et::H256::default();
    static ref EMPTY_ETH_NONCE: et::H64 = et::H64::default();

    // Keccak-256 of an RLP of an empty array
    static ref EMPTY_UNCLE_HASH: et::H256 = et::H256::from_slice(
        hex::decode("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347")
            .unwrap()
            .as_ref(),
    );

    // Keccak-256 hash of the RLP of null
    static ref EMPTY_ROOT_HASH: et::H256 = et::H256::from_slice(
        hex::decode("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421")
            .unwrap()
            .as_ref(),
    );

    static ref EMPTY_ETH_BLOOM: [u8; 2048/8] = [0u8; 2048/8];
    static ref FULL_ETH_BLOOM: [u8; 2048/8] = [0xff; 2048/8];

    static ref MAX_U256: BigInt = BigInt::from_str(&et::U256::MAX.to_string()).unwrap();

    pub static ref BLOCK_ZERO: tendermint::Block = block_zero();
    pub static ref BLOCK_ZERO_HASH: [u8; 32] = BLOCK_ZERO.header().hash().as_bytes().try_into().unwrap();
}

/// A pretend block at height 0 for some tools like The Graph which go there.
fn block_zero() -> tendermint::Block {
    let commit = tendermint::block::Commit {
        height: tendermint::block::Height::try_from(0u64).unwrap(),
        round: tendermint::block::Round::try_from(0).unwrap(),
        block_id: tendermint::block::Id {
            hash: tendermint::Hash::None,
            part_set_header: tendermint::block::parts::Header::new(0, tendermint::Hash::None)
                .unwrap(),
        },
        signatures: Vec::new(),
    };

    let empty_cid = fendermint_vm_message::cid(&[0u8; 0]).unwrap();

    let header = tendermint::block::Header {
        version: tendermint::block::header::Version { block: 0, app: 0 },
        chain_id: tendermint::chain::Id::try_from("UNSPECIFIED").expect("invalid chainid"),
        height: tendermint::block::Height::try_from(0u64).unwrap(),
        time: tendermint::time::Time::unix_epoch(),
        last_block_id: None,
        last_commit_hash: None,
        data_hash: None,
        validators_hash: tendermint::Hash::None,
        next_validators_hash: tendermint::Hash::None,
        consensus_hash: tendermint::Hash::None,
        app_hash: tendermint::AppHash::try_from(empty_cid.to_bytes()).unwrap(),
        last_results_hash: None,
        evidence_hash: None,
        proposer_address: tendermint::account::Id::new([0u8; 20]),
    };

    tendermint::Block::new(
        header,
        Vec::new(),
        // tendermint::evidence::Data::default(),
        tendermint::evidence::List::default(),
        Some(commit),
    )
    .unwrap()
}

pub fn is_block_zero(block: &tendermint::Block) -> bool {
    block.header().hash() == tendermint::Hash::Sha256(*BLOCK_ZERO_HASH)
}

/// Convert a Tendermint block to Ethereum with only the block hashes in the body.
pub fn to_eth_block(
    block: &tendermint::Block,
    block_results: tendermint_rpc::endpoint::block_results::Response,
    base_fee: TokenAmount,
    chain_id: ChainID,
) -> anyhow::Result<et::Block<et::Transaction>> {
    // Based on https://github.com/evmos/ethermint/blob/07cf2bd2b1ce9bdb2e44ec42a39e7239292a14af/rpc/types/utils.go#L113
    //          https://github.com/evmos/ethermint/blob/07cf2bd2b1ce9bdb2e44ec42a39e7239292a14af/rpc/backend/blocks.go#L365
    //          https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#L1883

    let hash = et::H256::from_slice(block.header().hash().as_ref());

    let parent_hash = if block.header.height.value() == 1 {
        // Just in case the client tool wants to compare hashes.
        et::H256::from_slice(BLOCK_ZERO_HASH.as_ref())
    } else {
        block
            .header()
            .last_block_id
            .map(|id| et::H256::from_slice(id.hash.as_bytes()))
            .unwrap_or_default()
    };

    let transactions_root = if block.data.is_empty() {
        *EMPTY_ROOT_HASH
    } else {
        block
            .header()
            .data_hash
            .map(|h| et::H256::from_slice(h.as_bytes()))
            .unwrap_or(*EMPTY_ROOT_HASH)
    };

    // Tendermint's account hash luckily has the same length as Eth.
    let author = et::H160::from_slice(block.header().proposer_address.as_bytes());

    let transaction_results = block_results.txs_results.unwrap_or_default();
    let mut transactions = Vec::new();
    let mut size = et::U256::zero();
    let mut gas_limit = et::U256::zero();
    let mut gas_used = et::U256::zero();

    // I'm just going to skip all the future message types here, which are CID based.
    // To deal with them, we'd have to send IPLD requests via ABCI to resolve them,
    // potentially through multiple hops. Let's leave that for the future and for now
    // assume that all we have is signed transactions.
    for (idx, data) in block.data().iter().enumerate() {
        let result = match transaction_results.get(idx) {
            Some(result) => result,
            None => continue,
        };

        size += et::U256::from(data.len());
        gas_used += et::U256::from(result.gas_used);
        gas_limit += et::U256::from(result.gas_wanted);

        let msg = to_chain_message(data)?;

        if let ChainMessage::Signed(msg) = msg {
            let hash = msg_hash(&result.events, data);

            let mut tx = to_eth_transaction(msg, chain_id, hash)
                .context("failed to convert to eth transaction")?;

            tx.transaction_index = Some(et::U64::from(idx));
            tx.block_hash = Some(et::H256::from_slice(block.header.hash().as_bytes()));
            tx.block_number = Some(et::U64::from(block.header.height.value()));

            transactions.push(tx);
        }
    }

    let block = et::Block {
        hash: Some(hash),
        parent_hash,
        number: Some(et::U64::from(block.header().height.value())),
        timestamp: et::U256::from(block.header().time.unix_timestamp()),
        author: Some(author),
        state_root: app_hash_to_root(&block.header().app_hash)?,
        transactions_root,
        base_fee_per_gas: Some(to_eth_tokens(&base_fee)?),
        difficulty: et::U256::zero(),
        total_difficulty: Some(et::U256::zero()),
        nonce: Some(*EMPTY_ETH_NONCE),
        mix_hash: Some(*EMPTY_ETH_HASH),
        uncles: Vec::new(),
        uncles_hash: *EMPTY_UNCLE_HASH,
        receipts_root: *EMPTY_ROOT_HASH,
        extra_data: et::Bytes::default(),
        logs_bloom: Some(et::Bloom::from_slice(&*EMPTY_ETH_BLOOM)),
        withdrawals_root: None,
        withdrawals: None,
        seal_fields: Vec::new(),
        other: Default::default(),
        transactions,
        size: Some(size),
        gas_limit,
        gas_used,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
    };

    Ok(block)
}

pub fn to_eth_transaction(
    msg: SignedMessage,
    chain_id: ChainID,
    hash: et::TxHash,
) -> anyhow::Result<et::Transaction> {
    // Based on https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#L2048
    let sig =
        to_eth_signature(msg.signature(), true).context("failed to convert to eth signature")?;

    // Recover the original request; this method has better tests.
    let tx = to_eth_transaction_request(&msg.message, &chain_id)
        .context("failed to convert to tx request")?;

    let tx = from_eth::to_eth_transaction(tx, sig, hash);

    Ok(tx)
}

/// Helper function to produce cumulative gas used after the execution of each transaction in a block,
/// along with cumulative event log count.
pub fn to_cumulative(block_results: &endpoint::block_results::Response) -> Vec<(et::U256, usize)> {
    let mut records = Vec::new();
    let mut cumulative_gas_used = et::U256::zero();
    let mut cumulative_event_count = 0usize;
    if let Some(rs) = block_results.txs_results.as_ref() {
        for r in rs {
            cumulative_gas_used += et::U256::from(r.gas_used);
            cumulative_event_count += r.events.len();
            records.push((cumulative_gas_used, cumulative_event_count));
        }
    }
    records
}

// https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#L2174
// https://github.com/evmos/ethermint/blob/07cf2bd2b1ce9bdb2e44ec42a39e7239292a14af/rpc/backend/tx_info.go#L147
pub async fn to_eth_receipt(
    msg: &SignedMessage,
    result: &endpoint::tx::Response,
    cumulative: &[(et::U256, usize)],
    header: &tendermint::block::Header,
    base_fee: &TokenAmount,
) -> anyhow::Result<et::TransactionReceipt> {
    let block_hash = et::H256::from_slice(header.hash().as_bytes());
    let block_number = et::U64::from(result.height.value());
    let transaction_index = et::U64::from(result.index);
    let transaction_hash = msg_hash(&result.tx_result.events, &result.tx);

    let msg = &msg.message;
    // Lotus effective gas price is based on total spend divided by gas used,
    // for which it recalculates the gas outputs. However, we don't have access
    // to the VM interpreter here to restore those results, and they are discarded
    // from the [`ApplyRet`] during the conversion to [`DeliverTx`].
    // We could put it into the [`DeliverTx::info`] field, or we can calculate
    // something based on the gas fields of the transaction, like Ethermint.
    let effective_gas_price =
        crate::gas::effective_gas_price(msg, base_fee, result.tx_result.gas_used);

    // Sum up gas up to this transaction.
    let (cumulative_gas_used, cumulative_event_count) = cumulative
        .get(result.index as usize)
        .cloned()
        .unwrap_or_default();

    let log_index_start = cumulative_event_count.saturating_sub(result.tx_result.events.len());

    let logs = to_logs(
        &result.tx_result.events,
        block_hash,
        block_number,
        transaction_hash,
        transaction_index,
        log_index_start,
    )
    .context("failed to collect logs")?;

    // See if the return value is an Ethereum contract creation.
    // https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#LL2240C9-L2240C15
    let contract_address = if result.tx_result.code.is_err() {
        None
    } else {
        maybe_contract_address(&result.tx_result).map(|ca| et::H160::from_slice(&ca.0))
    };

    let receipt = et::TransactionReceipt {
        transaction_hash,
        transaction_index,
        block_hash: Some(block_hash),
        block_number: Some(block_number),
        from: to_eth_address(&msg.from).ok().flatten().unwrap_or_default(),
        to: to_eth_address(&msg.to).ok().flatten(),
        cumulative_gas_used,
        gas_used: Some(et::U256::from(result.tx_result.gas_used)),
        contract_address,
        logs,
        status: Some(et::U64::from(if result.tx_result.code.is_ok() {
            1
        } else {
            0
        })),
        root: Some(app_hash_to_root(&header.app_hash)?),
        logs_bloom: et::Bloom::from_slice(&*EMPTY_ETH_BLOOM),
        transaction_type: Some(et::U64::from(2)), // Value used by Lotus.
        effective_gas_price: Some(to_eth_tokens(&effective_gas_price)?),
        other: Default::default(),
    };
    Ok(receipt)
}

/// Change the type of transactions in a block by mapping a function over them.
pub fn map_rpc_block_txs<F, A, B, E>(block: et::Block<A>, f: F) -> Result<et::Block<B>, E>
where
    F: Fn(A) -> Result<B, E>,
{
    let et::Block {
        hash,
        parent_hash,
        uncles_hash,
        author,
        state_root,
        transactions_root,
        receipts_root,
        number,
        gas_used,
        gas_limit,
        extra_data,
        logs_bloom,
        timestamp,
        difficulty,
        total_difficulty,
        seal_fields,
        uncles,
        transactions,
        size,
        mix_hash,
        nonce,
        base_fee_per_gas,
        withdrawals_root,
        withdrawals,
        other,
        blob_gas_used,
        excess_blob_gas,
        parent_beacon_block_root,
    } = block;

    let transactions: Result<Vec<B>, E> = transactions.into_iter().map(f).collect();
    let transactions = transactions?;

    let block = et::Block {
        hash,
        parent_hash,
        uncles_hash,
        author,
        state_root,
        transactions_root,
        receipts_root,
        number,
        gas_used,
        gas_limit,
        extra_data,
        logs_bloom,
        timestamp,
        difficulty,
        total_difficulty,
        seal_fields,
        uncles,
        size,
        mix_hash,
        nonce,
        base_fee_per_gas,
        withdrawals_root,
        withdrawals,
        transactions,
        other,
        blob_gas_used,
        excess_blob_gas,
        parent_beacon_block_root,
    };

    Ok(block)
}

fn app_hash_to_root(app_hash: &tendermint::AppHash) -> anyhow::Result<et::H256> {
    // Out app hash is a CID. We only need the hash part.
    // Actually it's not the state root of the actors, but it's still a CID.
    let state_root = cid::Cid::try_from(app_hash.as_bytes()).context("app hash is not a CID")?;
    // Just in case we returned `Cid::default()`
    if state_root.hash().digest().is_empty() {
        Ok(et::H256::default())
    } else {
        Ok(et::H256::from_slice(state_root.hash().digest()))
    }
}

fn maybe_contract_address(deliver_tx: &ExecTxResult) -> Option<EthAddress> {
    fendermint_rpc::response::decode_fevm_create(deliver_tx)
        .ok()
        .map(|cr| {
            // We can return either `cr.actor_id` as a masked address,
            // or `cr.eth_address`. Both addresses are usable for calling the contract.
            // However, the masked ID doesn't work with some of the Ethereum tooling which check some hash properties.
            // We also have to make sure to use the same kind of address that we do in the filtering and event logs,
            // otherwise the two doesn't align and it makes the API difficult to use. It's impossible(?) to find out
            // the actor ID just using the Ethereum API, so best use the same.
            // EthAddress::from_id(cr.actor_id)
            cr.eth_address
        })
}

/// Artificial block-zero.
pub fn to_eth_block_zero(block: tendermint::Block) -> anyhow::Result<et::Block<serde_json::Value>> {
    let block_results = tendermint_rpc::endpoint::block_results::Response {
        height: block.header.height,
        txs_results: None,
        begin_block_events: None,
        end_block_events: None,
        validator_updates: Vec::new(),
        consensus_param_updates: None,
        finalize_block_events: Vec::new(),
        app_hash: Default::default(),
    };
    let block = to_eth_block(&block, block_results, TokenAmount::zero(), ChainID::from(0))
        .context("failed to map block zero to eth")?;
    let block =
        map_rpc_block_txs(block, serde_json::to_value).context("failed to convert to JSON")?;
    Ok(block)
}

/// Turn Events into Ethereum logs.
///
/// We need to turn Actor IDs into Ethereum addresses because that's what the tooling expects.
pub fn to_logs(
    events: &[abci::Event],
    block_hash: et::H256,
    block_number: et::U64,
    transaction_hash: et::H256,
    transaction_index: et::U64,
    log_index_start: usize,
) -> anyhow::Result<Vec<et::Log>> {
    let mut logs = Vec::new();
    for (idx, event) in events.iter().filter(|e| e.kind == "event").enumerate() {
        // Lotus looks up an Ethereum address based on the actor ID:
        // https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#L1987

        let addr = event
            .attributes
            .iter()
            .find(|a| matches!(a.key_str(), Ok(key_str) if key_str == "emitter.deleg"))
            .and_then(|a| a.value_str().ok())
            .and_then(|a| a.parse::<Address>().ok());

        let actor_id = event
            .attributes
            .iter()
            .find(|a| matches!(a.key_str(), Ok(key_str) if key_str == "emitter.id"))
            .and_then(|a| a.value_str().ok())
            .and_then(|a| a.parse::<u64>().ok())
            .ok_or_else(|| anyhow!("cannot find the 'emitter.id' key"))?;

        let address = addr
            .and_then(|a| to_eth_address(&a).ok())
            .flatten()
            .unwrap_or_else(|| et::H160::from(EthAddress::from_id(actor_id).0));

        // https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#LL2240C9-L2240C15
        let (topics, data) =
            to_topics_and_data(&event.attributes).context("failed to collect topics and data")?;

        // Blockscout doesn't recognise the `logType` field since https://github.com/blockscout/blockscout/pull/9007
        let log_type = None; // Some(event.kind.clone()),

        let log = et::Log {
            address,
            topics,
            data,
            block_hash: Some(block_hash),
            block_number: Some(block_number),
            transaction_hash: Some(transaction_hash),
            transaction_index: Some(transaction_index),
            log_index: Some(et::U256::from(idx + log_index_start)),
            transaction_log_index: Some(et::U256::from(idx)),
            log_type,
            removed: Some(false),
        };

        logs.push(log);
    }
    Ok(logs)
}

// Find the Ethereum topics (up to 4) and the data in the event attributes.
fn to_topics_and_data(attrs: &Vec<EventAttribute>) -> anyhow::Result<(Vec<et::H256>, et::Bytes)> {
    // Based on https://github.com/filecoin-project/lotus/blob/6cc506f5cf751215be6badc94a960251c6453202/node/impl/full/eth.go#L1534
    let mut topics = Vec::new();
    let mut data = None;
    for attr in attrs {
        let decode_value = || {
            hex::decode(attr.value_str()?).with_context(|| {
                format!(
                    "failed to decode attr value as hex: {}",
                    &attr.value_str().unwrap()
                )
            })
        };

        match attr.key_str()? {
            "t1" | "t2" | "t3" | "t4" => {
                let bz = decode_value()?;
                if bz.len() != 32 {
                    return Err(anyhow!("unexpected topic value: {attr:?}"));
                }
                let h = et::H256::from_slice(&bz);
                let i = attr.key_str()?[1..]
                    .parse::<usize>()
                    .unwrap()
                    .saturating_sub(1);
                while topics.len() <= i {
                    topics.push(et::H256::default())
                }
                topics[i] = h;
            }
            "d" => data = Some(et::Bytes::from(decode_value()?)),
            _ => {} // e.g. "emitter.*"
        }
    }
    Ok((topics, data.unwrap_or_default()))
}

/// Decode the transaction payload as a [ChainMessage].
pub fn to_chain_message(tx: &[u8]) -> anyhow::Result<ChainMessage> {
    fvm_ipld_encoding::from_slice::<ChainMessage>(tx).context("failed to decode tx as ChainMessage")
}

/// Hash the transaction payload the way Tendermint does,
/// to calculate the transaction hash which is otherwise unavailable.
///
/// This is here for reference only and should not be returned to Ethereum tools which expect
/// the hash to be based on RLP and Keccak256.
pub fn tx_hash(tx: &[u8]) -> tendermint::Hash {
    // based on how `tendermint::Header::hash` works.
    let hash = tendermint::crypto::default::Sha256::digest(tx);
    tendermint::Hash::Sha256(hash)
}

/// Best effort to find and parse any `<kind>.hash` attribute emitted among the events.
pub fn find_hash_event(kind: &str, events: &[abci::Event]) -> Option<et::H256> {
    events
        .iter()
        .find(|e| e.kind == kind)
        .and_then(|e| {
            e.attributes
                .iter()
                .find(|a| matches!(a.key_str(), Ok(key_str) if key_str == "hash"))
        })
        .and_then(|a| hex::decode(a.value_str().unwrap()).ok())
        .filter(|bz| bz.len() == 32)
        .map(|bz| et::H256::from_slice(&bz))
}

// Calculate some kind of hash for the message, preferrably one the tools expect.
pub fn msg_hash(events: &[Event], tx: &[u8]) -> et::TxHash {
    if let Some(h) = find_hash_event("eth", events) {
        h
    } else {
        // Return the default hash, at least there is something
        et::TxHash::from_slice(tx_hash(tx).as_bytes())
    }
}

/// Collect and parse all `emitter.deleg` or `emitter.id` in the events.
pub fn collect_emitters(events: &[abci::Event]) -> HashSet<Address> {
    let mut emitters = HashSet::new();
    for event in events.iter().filter(|e| e.kind == "event") {
        for addr in [
            event
                .attributes
                .iter()
                .find(|a| matches!(a.key_str(), Ok(key_str) if key_str == "emitter.deleg"))
                .and_then(|a| a.value_str().unwrap().parse::<Address>().ok()),
            event
                .attributes
                .iter()
                .find(|a| matches!(a.key_str(), Ok(key_str) if key_str == "emitter.id"))
                .and_then(|a| a.value_str().unwrap().parse::<u64>().ok())
                .map(Address::new_id),
        ]
        .into_iter()
        .flatten()
        {
            emitters.insert(addr);
        }
    }
    emitters
}

#[cfg(test)]
mod tests {
    use crate::conv::from_tm::is_block_zero;

    use super::{to_eth_block_zero, BLOCK_ZERO};

    #[test]
    fn block_zero_can_be_created() {
        assert!(is_block_zero(&BLOCK_ZERO))
    }

    #[test]
    fn block_zero_can_be_turned_into_eth() {
        let _ = to_eth_block_zero(BLOCK_ZERO.clone()).unwrap();
    }
}
