// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

// See the following for inspiration:
// * https://github.com/evmos/ethermint/blob/ebbe0ffd0d474abd745254dc01e60273ea758dae/rpc/namespaces/ethereum/eth/api.go#L44
// * https://github.com/filecoin-project/lotus/blob/v1.23.1-rc2/api/api_full.go#L783
// * https://github.com/filecoin-project/lotus/blob/v1.23.1-rc2/node/impl/full/eth.go

use std::collections::HashSet;

use anyhow::Context;
use ethers_core::types::transaction::eip2718::TypedTransaction;
use ethers_core::types::{self as et, BlockNumber};
use ethers_core::utils::rlp;
use fendermint_rpc::message::SignedMessageFactory;
use fendermint_rpc::query::QueryClient;
use fendermint_rpc::response::{decode_data, decode_fevm_invoke, decode_fevm_return_data};
use fendermint_vm_actor_interface::eam::{EthAddress, EAM_ACTOR_ADDR};
use fendermint_vm_actor_interface::evm;
use fendermint_vm_message::chain::ChainMessage;
use fendermint_vm_message::query::FvmQueryHeight;
use fendermint_vm_message::signed::SignedMessage;
use futures::FutureExt;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::{chainid::ChainID, error::ExitCode};
use jsonrpc_v2::Params;
use rand::Rng;
use tendermint::block::Height;
use tendermint_rpc::endpoint::{self, status};
use tendermint_rpc::SubscriptionClient;
use tendermint_rpc::{
    endpoint::{block, block_results, broadcast::tx_sync, consensus_params, header},
    Client,
};

use fil_actors_evm_shared::uints;

use crate::conv::from_eth::{self, to_fvm_message};
use crate::conv::from_tm::{self, msg_hash, to_chain_message, to_cumulative, to_eth_block_zero};
use crate::error::{error_with_revert, OutOfSequence};
use crate::filters::{matches_topics, FilterId, FilterKind, FilterRecords};
use crate::{
    conv::{
        from_eth::to_fvm_address,
        from_fvm::to_eth_tokens,
        from_tm::{to_eth_receipt, to_eth_transaction},
    },
    error, JsonRpcData, JsonRpcResult,
};

/// Returns a list of addresses owned by client.
///
/// It will always return [] since we don't expect Fendermint to manage private keys.
pub async fn accounts<C>(_data: JsonRpcData<C>) -> JsonRpcResult<Vec<et::Address>> {
    Ok(vec![])
}

/// Returns the number of most recent block.
pub async fn block_number<C>(data: JsonRpcData<C>) -> JsonRpcResult<et::U64>
where
    C: Client + Sync + Send,
{
    let height = data.latest_height().await?;
    Ok(et::U64::from(height.value()))
}

/// Returns the chain ID used for signing replay-protected transactions.
pub async fn chain_id<C>(data: JsonRpcData<C>) -> JsonRpcResult<et::U64>
where
    C: Client + Sync + Send,
{
    let res = data.client.state_params(FvmQueryHeight::default()).await?;
    Ok(et::U64::from(res.value.chain_id))
}

/// The current FVM network version.
pub async fn protocol_version<C>(data: JsonRpcData<C>) -> JsonRpcResult<String>
where
    C: Client + Sync + Send,
{
    let res = data.client.state_params(FvmQueryHeight::default()).await?;
    let version: u32 = res.value.network_version.into();
    Ok(version.to_string())
}

/// Returns a fee per gas that is an estimate of how much you can pay as a
/// priority fee, or 'tip', to get a transaction included in the current block.
pub async fn max_priority_fee_per_gas<C>(data: JsonRpcData<C>) -> JsonRpcResult<et::U256>
where
    C: Client + Sync + Send,
{
    // get the latest block
    let res: block::Response = data.tm().latest_block().await?;
    let latest_h = res.block.header.height;

    // get consensus params to fetch block gas limit
    // (this just needs to be done once as we assume that is constant
    // for all blocks)
    let consensus_params: consensus_params::Response = data
        .tm()
        .consensus_params(latest_h)
        .await
        .context("failed to get consensus params")?;
    let mut block_gas_limit = consensus_params.consensus_params.block.max_gas;
    if block_gas_limit <= 0 {
        block_gas_limit =
            i64::try_from(fvm_shared::BLOCK_GAS_LIMIT).expect("FVM block gas limit not i64")
    };

    let mut premiums = Vec::new();
    // iterate through the blocks in the range
    // we may be able to de-duplicate a lot of this code from fee_history
    let latest_h: u64 = latest_h.into();
    let mut blk = latest_h;
    while blk > latest_h - data.gas_opt.num_blocks_max_prio_fee {
        let block = data
            .block_by_height(blk.into())
            .await
            .context("failed to get block")?;

        let height = block.header().height;

        // Genesis has height 1, but no relevant fees.
        if height.value() <= 1 {
            break;
        }

        let state_params = data
            .client
            .state_params(FvmQueryHeight::Height(height.value()))
            .await?;

        let base_fee = &state_params.value.base_fee;

        // The latest block might not have results yet.
        if let Ok(block_results) = data.tm().block_results(height).await {
            let txs_results = block_results.txs_results.unwrap_or_default();

            for (tx, txres) in block.data().iter().zip(txs_results) {
                let msg = fvm_ipld_encoding::from_slice::<ChainMessage>(tx)
                    .context("failed to decode tx as ChainMessage")?;

                if let ChainMessage::Signed(msg) = msg {
                    let premium = crate::gas::effective_gas_premium(&msg.message, base_fee);
                    premiums.push((premium, txres.gas_used));
                }
            }
        }
        blk -= 1;
    }

    // compute median gas price
    let mut median = crate::gas::median_gas_premium(&mut premiums, block_gas_limit);
    let min_premium = data.gas_opt.min_gas_premium.clone();
    if median < min_premium {
        median = min_premium;
    }

    // add some noise to normalize behaviour of message selection
    // mean 1, stddev 0.005 => 95% within +-1%
    const PRECISION: u32 = 32;
    let mut rng = rand::thread_rng();
    let noise: f64 = 1.0 + rng.gen::<f64>() * 0.005;
    let precision: i64 = 32;
    let coeff: u64 = ((noise * (1 << precision) as f64) as u64) + 1;

    median *= BigInt::from(coeff);
    median.div_ceil(BigInt::from(1 << PRECISION));

    Ok(to_eth_tokens(&median)?)
}

/// Returns transaction base fee per gas and effective priority fee per gas for the requested/supported block range.
pub async fn fee_history<C>(
    data: JsonRpcData<C>,
    Params((block_count, last_block, reward_percentiles)): Params<(
        et::U256,
        et::BlockNumber,
        Vec<f64>,
    )>,
) -> JsonRpcResult<et::FeeHistory>
where
    C: Client + Sync + Send,
{
    if block_count > et::U256::from(data.gas_opt.max_fee_hist_size) {
        return error(
            ExitCode::USR_ILLEGAL_ARGUMENT,
            "block_count must be <= 1024",
        );
    }

    let mut hist = et::FeeHistory {
        base_fee_per_gas: Vec::new(),
        gas_used_ratio: Vec::new(),
        oldest_block: et::U256::default(),
        reward: Vec::new(),
    };
    let mut block_number = last_block;
    let mut block_count = block_count.as_usize();

    let get_base_fee = |height: Height| {
        data.client
            .state_params(FvmQueryHeight::Height(height.value()))
            .map(|result| result.map(|state_params| state_params.value.base_fee))
    };

    while block_count > 0 {
        let block = data
            .block_by_height(block_number)
            .await
            .context("failed to get block")?;

        let height = block.header().height;

        // Apparently the base fees have to include the next fee after the newest block.
        // See https://github.com/filecoin-project/lotus/blob/v1.25.2/node/impl/full/eth.go#L721-L725
        if hist.base_fee_per_gas.is_empty() {
            let base_fee = get_base_fee(height.increment())
                .await
                .context("failed to get next base fee")?;

            hist.base_fee_per_gas.push(to_eth_tokens(&base_fee)?);
        }

        let base_fee = get_base_fee(height)
            .await
            .context("failed to get block base fee")?;

        let consensus_params: consensus_params::Response = data
            .tm()
            .consensus_params(height)
            .await
            .context("failed to get consensus params")?;

        let mut block_gas_limit = consensus_params.consensus_params.block.max_gas;
        if block_gas_limit <= 0 {
            block_gas_limit =
                i64::try_from(fvm_shared::BLOCK_GAS_LIMIT).expect("FVM block gas limit not i64")
        };

        // The latest block might not have results yet.
        if let Ok(block_results) = data.tm().block_results(height).await {
            let txs_results = block_results.txs_results.unwrap_or_default();
            let total_gas_used: i64 = txs_results.iter().map(|r| r.gas_used).sum();

            let mut premiums = Vec::new();
            for (tx, txres) in block.data().iter().zip(txs_results) {
                let msg = fvm_ipld_encoding::from_slice::<ChainMessage>(tx)
                    .context("failed to decode tx as ChainMessage")?;

                if let ChainMessage::Signed(msg) = msg {
                    let premium = crate::gas::effective_gas_premium(&msg.message, &base_fee);
                    premiums.push((premium, txres.gas_used));
                }
            }
            premiums.sort();

            let premium_gas_used: i64 = premiums.iter().map(|(_, gas)| *gas).sum();

            let rewards: Result<Vec<et::U256>, _> = reward_percentiles
                .iter()
                .map(|p| {
                    if premiums.is_empty() {
                        Ok(et::U256::zero())
                    } else {
                        let threshold_gas_used = (premium_gas_used as f64 * p / 100f64) as i64;
                        let mut sum_gas_used = 0;
                        let mut idx = 0;
                        while sum_gas_used < threshold_gas_used && idx < premiums.len() - 1 {
                            sum_gas_used += premiums[idx].1;
                            idx += 1;
                        }
                        to_eth_tokens(&premiums[idx].0)
                    }
                })
                .collect();

            hist.oldest_block = et::U256::from(height.value());
            hist.base_fee_per_gas.push(to_eth_tokens(&base_fee)?);
            hist.gas_used_ratio
                .push(total_gas_used as f64 / block_gas_limit as f64);
            hist.reward.push(rewards?);

            block_count -= 1;
        }

        // Genesis has height 1.
        if height.value() <= 1 {
            break;
        }

        block_number = et::BlockNumber::Number(et::U64::from(height.value() - 1));
    }

    // Reverse data to be oldest-to-newest.
    hist.base_fee_per_gas.reverse();
    hist.gas_used_ratio.reverse();
    hist.reward.reverse();

    Ok(hist)
}

/// Returns the current price per gas in wei.
pub async fn gas_price<C>(data: JsonRpcData<C>) -> JsonRpcResult<et::U256>
where
    C: Client + Sync + Send,
{
    let res = data.client.state_params(FvmQueryHeight::default()).await?;
    let price = to_eth_tokens(&res.value.base_fee)?;
    Ok(price)
}

/// Returns the balance of the account of given address.
pub async fn get_balance<C>(
    data: JsonRpcData<C>,
    Params((addr, block_id)): Params<(et::Address, et::BlockId)>,
) -> JsonRpcResult<et::U256>
where
    C: Client + Sync + Send,
{
    let addr = to_fvm_address(addr);
    let height = data.query_height(block_id).await?;
    let res = data.client.actor_state(&addr, height).await?;

    match res.value {
        Some((_, state)) => Ok(to_eth_tokens(&state.balance)?),
        None => Ok(et::U256::zero()),
    }
}

/// Returns information about a block by hash.
pub async fn get_block_by_hash<C>(
    data: JsonRpcData<C>,
    Params((block_hash, full_tx)): Params<(et::H256, bool)>,
) -> JsonRpcResult<Option<et::Block<serde_json::Value>>>
where
    C: Client + Sync + Send,
{
    match data.block_by_hash_opt(block_hash).await? {
        Some(block) if from_tm::is_block_zero(&block) => Ok(Some(to_eth_block_zero(block)?)),
        Some(block) => data.enrich_block(block, full_tx).await.map(Some),
        None => Ok(None),
    }
}

/// Returns information about a block by block number.
pub async fn get_block_by_number<C>(
    data: JsonRpcData<C>,
    Params((block_number, full_tx)): Params<(et::BlockNumber, bool)>,
) -> JsonRpcResult<Option<et::Block<serde_json::Value>>>
where
    C: Client + Sync + Send,
{
    match data.block_by_height(block_number).await? {
        block if block.header().height.value() > 0 => {
            data.enrich_block(block, full_tx).await.map(Some)
        }
        block if from_tm::is_block_zero(&block) => Ok(Some(to_eth_block_zero(block)?)),
        _ => Ok(None),
    }
}

/// Returns the number of transactions in a block matching the given block number.
pub async fn get_block_transaction_count_by_number<C>(
    data: JsonRpcData<C>,
    Params((block_number,)): Params<(et::BlockNumber,)>,
) -> JsonRpcResult<et::U64>
where
    C: Client + Sync + Send,
{
    let block = data.block_by_height(block_number).await?;

    Ok(et::U64::from(block.data.len()))
}

/// Returns the number of transactions in a block from a block matching the given block hash.
pub async fn get_block_transaction_count_by_hash<C>(
    data: JsonRpcData<C>,
    Params((block_hash,)): Params<(et::H256,)>,
) -> JsonRpcResult<et::U64>
where
    C: Client + Sync + Send,
{
    let block = data.block_by_hash_opt(block_hash).await?;
    let count = block
        .map(|b| et::U64::from(b.data.len()))
        .unwrap_or_default();
    Ok(count)
}

/// Returns the information about a transaction requested by transaction hash.
pub async fn get_transaction_by_block_hash_and_index<C>(
    data: JsonRpcData<C>,
    Params((block_hash, index)): Params<(et::H256, et::U64)>,
) -> JsonRpcResult<Option<et::Transaction>>
where
    C: Client + Sync + Send,
{
    if let Some(block) = data.block_by_hash_opt(block_hash).await? {
        data.transaction_by_index(block, index).await
    } else {
        Ok(None)
    }
}

/// Returns the information about a transaction requested by transaction hash.
pub async fn get_transaction_by_block_number_and_index<C>(
    data: JsonRpcData<C>,
    Params((block_number, index)): Params<(et::BlockNumber, et::U64)>,
) -> JsonRpcResult<Option<et::Transaction>>
where
    C: Client + Sync + Send,
{
    let block = data.block_by_height(block_number).await?;
    data.transaction_by_index(block, index).await
}

/// Returns the information about a transaction requested by transaction hash.
pub async fn get_transaction_by_hash<C>(
    data: JsonRpcData<C>,
    Params((tx_hash,)): Params<(et::H256,)>,
) -> JsonRpcResult<Option<et::Transaction>>
where
    C: Client + Sync + Send,
{
    // Check in the pending cache first.
    if let Some(tx) = data.tx_cache.get(&tx_hash) {
        Ok(Some(tx))
    } else if let Some(res) = data.tx_by_hash(tx_hash).await? {
        let msg = to_chain_message(&res.tx)?;

        if let ChainMessage::Signed(msg) = msg {
            let header: header::Response = data.tm().header(res.height).await?;
            let sp = data
                .client
                .state_params(FvmQueryHeight::Height(header.header.height.value()))
                .await?;
            let chain_id = ChainID::from(sp.value.chain_id);
            let hash = msg_hash(&res.tx_result.events, &res.tx);
            let mut tx = to_eth_transaction(msg, chain_id, hash)?;
            tx.transaction_index = Some(et::U64::from(res.index));
            tx.block_hash = Some(et::H256::from_slice(header.header.hash().as_bytes()));
            tx.block_number = Some(et::U64::from(res.height.value()));
            Ok(Some(tx))
        } else {
            error(ExitCode::USR_ILLEGAL_ARGUMENT, "incompatible transaction")
        }
    } else {
        Ok(None)
    }
}

/// Returns the number of transactions sent from an address, up to a specific block.
///
/// This is done by looking up the nonce of the account.
pub async fn get_transaction_count<C>(
    data: JsonRpcData<C>,
    Params((addr, block_id)): Params<(et::Address, et::BlockId)>,
) -> JsonRpcResult<et::U64>
where
    C: Client + Sync + Send,
{
    let addr = to_fvm_address(addr);
    let height = data.query_height(block_id).await?;
    let res = data.client.actor_state(&addr, height).await?;

    match res.value {
        Some((_, state)) => {
            let nonce = state.sequence;
            Ok(et::U64::from(nonce))
        }
        None => Ok(et::U64::zero()),
    }
}

/// Returns the receipt of a transaction by transaction hash.
pub async fn get_transaction_receipt<C>(
    data: JsonRpcData<C>,
    Params((tx_hash,)): Params<(et::H256,)>,
) -> JsonRpcResult<Option<et::TransactionReceipt>>
where
    C: Client + Sync + Send,
{
    if let Some(res) = data.tx_by_hash(tx_hash).await? {
        let header: header::Response = data.tm().header(res.height).await?;
        let block_results: block_results::Response = data.tm().block_results(res.height).await?;
        let cumulative = to_cumulative(&block_results);
        let state_params = data
            .client
            .state_params(FvmQueryHeight::Height(header.header.height.value()))
            .await?;
        let msg = to_chain_message(&res.tx)?;
        if let ChainMessage::Signed(msg) = msg {
            let receipt = to_eth_receipt(
                &msg,
                &res,
                &cumulative,
                &header.header,
                &state_params.value.base_fee,
            )
            .await
            .context("failed to convert to receipt")?;

            Ok(Some(receipt))
        } else {
            error(ExitCode::USR_ILLEGAL_ARGUMENT, "incompatible transaction")
        }
    } else {
        Ok(None)
    }
}

/// Returns receipts for all the transactions in a block.
pub async fn get_block_receipts<C>(
    data: JsonRpcData<C>,
    Params((block_number,)): Params<(et::BlockNumber,)>,
) -> JsonRpcResult<Vec<et::TransactionReceipt>>
where
    C: Client + Sync + Send,
{
    let block = data.block_by_height(block_number).await?;
    if from_tm::is_block_zero(&block) {
        return Ok(Vec::new());
    }
    let height = block.header.height;
    let state_params = data
        .client
        .state_params(FvmQueryHeight::Height(height.value()))
        .await?;
    let block_results: block_results::Response = data.tm().block_results(height).await?;
    let cumulative = to_cumulative(&block_results);
    let mut receipts = Vec::new();

    for (index, (tx, tx_result)) in block
        .data
        .into_iter()
        .zip(block_results.txs_results.unwrap_or_default())
        .enumerate()
    {
        let msg = to_chain_message(&tx)?;
        if let ChainMessage::Signed(msg) = msg {
            let result = endpoint::tx::Response {
                hash: Default::default(), // Shouldn't use this anyway.
                height,
                index: index as u32,
                tx_result,
                tx,
                proof: None,
            };

            let receipt = to_eth_receipt(
                &msg,
                &result,
                &cumulative,
                &block.header,
                &state_params.value.base_fee,
            )
            .await?;
            receipts.push(receipt)
        }
    }
    Ok(receipts)
}

/// Returns the number of uncles in a block from a block matching the given block hash.
///
/// It will always return 0 since Tendermint doesn't have uncles.
pub async fn get_uncle_count_by_block_hash<C>(
    _data: JsonRpcData<C>,
    _params: Params<(et::H256,)>,
) -> JsonRpcResult<et::U256> {
    Ok(et::U256::zero())
}

/// Returns the number of uncles in a block from a block matching the given block number.
///
/// It will always return 0 since Tendermint doesn't have uncles.
pub async fn get_uncle_count_by_block_number<C>(
    _data: JsonRpcData<C>,
    _params: Params<(et::BlockNumber,)>,
) -> JsonRpcResult<et::U256> {
    Ok(et::U256::zero())
}

/// Returns information about a uncle of a block by hash and uncle index position.
///
/// It will always return None since Tendermint doesn't have uncles.
pub async fn get_uncle_by_block_hash_and_index<C>(
    _data: JsonRpcData<C>,
    _params: Params<(et::H256, et::U64)>,
) -> JsonRpcResult<Option<et::Block<et::H256>>> {
    Ok(None)
}

/// Returns information about a uncle of a block by number and uncle index position.
///
/// It will always return None since Tendermint doesn't have uncles.
pub async fn get_uncle_by_block_number_and_index<C>(
    _data: JsonRpcData<C>,
    _params: Params<(et::BlockNumber, et::U64)>,
) -> JsonRpcResult<Option<et::Block<et::H256>>> {
    Ok(None)
}

/// Creates new message call transaction or a contract creation for signed transactions.
pub async fn send_raw_transaction<C>(
    data: JsonRpcData<C>,
    Params((tx,)): Params<(et::Bytes,)>,
) -> JsonRpcResult<et::TxHash>
where
    C: Client + Sync + Send,
{
    let rlp = rlp::Rlp::new(tx.as_ref());
    let (tx, sig): (TypedTransaction, et::Signature) = TypedTransaction::decode_signed(&rlp)
        .context("failed to decode RLP as signed TypedTransaction")?;

    let sighash = tx.sighash();
    let msghash = et::TxHash::from(ethers_core::utils::keccak256(rlp.as_raw()));
    tracing::debug!(?sighash, eth_hash = ?msghash, ?tx, "received raw transaction");

    if let Some(tx) = tx.as_eip1559_ref() {
        let tx = from_eth::to_eth_transaction(tx.clone(), sig, msghash);
        data.tx_cache.insert(msghash, tx);
    }

    let msg = to_fvm_message(tx, false)?;
    let sender = msg.from;
    let nonce = msg.sequence;

    let msg = SignedMessage {
        message: msg,
        signature: Signature::new_secp256k1(sig.to_vec()),
    };
    let msg = ChainMessage::Signed(msg);
    let bz: Vec<u8> = SignedMessageFactory::serialize(&msg)?;

    // Use the broadcast version which waits for basic checks to complete,
    // but not the execution results - those will have to be polled with get_transaction_receipt.
    let res: tx_sync::Response = data.tm().broadcast_tx_sync(bz).await?;
    if res.code.is_ok() {
        // The following hash would be okay for ethers-rs,and we could use it to look up the TX with Tendermint,
        // but ethers.js would reject it because it doesn't match what Ethereum would use.
        // Ok(et::TxHash::from_slice(res.hash.as_bytes()))
        Ok(msghash)
    } else {
        // Try to decode any errors returned in the data.
        let bz = RawBytes::from(res.data.to_vec());
        // Might have to first call `decode_fevm_data` here in case CometBFT
        // wraps the data into Base64 encoding like it does for `DeliverTx`.
        let bz = decode_fevm_return_data(bz)
            .or_else(|_| decode_data(&res.data).and_then(decode_fevm_return_data))
            .ok();

        let exit_code = ExitCode::new(res.code.value());

        // NOTE: We could have checked up front if we have buffered transactions already waiting,
        // in which case this have just been appended to the list.
        if let Some(oos) = OutOfSequence::try_parse(exit_code, &res.log) {
            let is_admissible = oos.is_admissible(data.max_nonce_gap);

            tracing::debug!(eth_hash = ?msghash, expected = oos.expected, got = oos.got, is_admissible, "out-of-sequence transaction received");

            if is_admissible {
                data.tx_buffer.insert(sender, nonce, msg);
                return Ok(msghash);
            }
        }

        error_with_revert(exit_code, res.log, bz)
    }
}

/// Executes a new message call immediately without creating a transaction on the block chain.
pub async fn call<C>(
    data: JsonRpcData<C>,
    Params((tx, block_id)): Params<(TypedTransactionCompat, et::BlockId)>,
) -> JsonRpcResult<et::Bytes>
where
    C: Client + Sync + Send,
{
    let msg = to_fvm_message(tx.into(), true)?;
    let is_create = msg.to == EAM_ACTOR_ADDR;
    let height = data.query_height(block_id).await?;
    let response = data.client.call(msg, height).await?;
    let deliver_tx = response.value;

    // Based on Lotus, we should return the data from the receipt.
    if deliver_tx.code.is_err() {
        // There might be some revert data encoded as ABI in the response.
        let (msg, data) = match decode_fevm_invoke(&deliver_tx) {
            Ok(h) => (deliver_tx.info, Some(h)),
            Err(e) => (
                format!("{}\nfailed to decode return data: {:#}", deliver_tx.info, e),
                None,
            ),
        };
        error_with_revert(ExitCode::new(deliver_tx.code.value()), msg, data)
    } else if is_create {
        // It's not clear why some tools like Remix call this with deployment transaction, but they do.
        // We could parse the deployed contract address, but it would be of very limited use;
        // the call effect isn't persisted, so one would have to send an actual transaction
        // and then run a call on `pending` state with this address to have a chance to hit
        // that contract before the transaction is included in a block, assuming address
        // creation is deterministic.
        // Lotus returns empty: https://github.com/filecoin-project/lotus/blob/v1.23.1-rc2/node/impl/full/eth.go#L1091-L1094
        Ok(Default::default())
    } else {
        let return_data = decode_fevm_invoke(&deliver_tx)
            .context("error decoding data from deliver_tx in query")?;
        Ok(return_data.into())
    }
}

/// Generates and returns an estimate of how much gas is necessary to allow the transaction to complete.
/// The transaction will not be added to the blockchain.
/// Note that the estimate may be significantly more than the amount of gas actually used by the transaction, f
/// or a variety of reasons including EVM mechanics and node performance.
pub async fn estimate_gas<C>(
    data: JsonRpcData<C>,
    Params(params): Params<EstimateGasParams>,
) -> JsonRpcResult<et::U256>
where
    C: Client + Sync + Send,
{
    let (tx, block_id) = match params {
        EstimateGasParams::One((tx,)) => (tx, et::BlockId::Number(et::BlockNumber::Latest)),
        EstimateGasParams::Two((tx, block_id)) => (tx, block_id),
    };

    let msg = to_fvm_message(tx.into(), true).context("failed to convert to FVM message")?;

    let height = data
        .query_height(block_id)
        .await
        .context("failed to get height")?;

    let response = data
        .client
        .estimate_gas(msg, height)
        .await
        .context("failed to call estimate gas query")?;

    let estimate = response.value;

    if !estimate.exit_code.is_success() {
        // There might be some revert data encoded as ABI in the response.
        let msg = format!("failed to estimate gas: {}", estimate.info);
        let (msg, data) = match decode_fevm_return_data(estimate.return_data) {
            Ok(h) => (msg, Some(h)),
            Err(e) => (format!("{msg}\n{e:#}"), None),
        };
        error_with_revert(estimate.exit_code, msg, data)
    } else {
        Ok(estimate.gas_limit.into())
    }
}

/// Returns the value from a storage position at a given address.
///
/// The return value is a hex encoded U256.
pub async fn get_storage_at<C>(
    data: JsonRpcData<C>,
    Params((address, position, block_id)): Params<(et::H160, et::U256, et::BlockId)>,
) -> JsonRpcResult<String>
where
    C: Client + Sync + Send,
{
    let encode = |data: Option<uints::U256>| {
        let mut bz = [0u8; 32];
        if let Some(data) = data {
            data.to_big_endian(&mut bz);
        }
        // The client library expects hex encoded string. The JS client might want a prefix too.
        Ok(format!("0x{}", hex::encode(bz)))
    };

    let height = data.query_height(block_id).await?;

    // If not an EVM actor, return empty.
    if data.get_actor_type(&address, height).await? != ActorType::EVM {
        // The client library expects hex encoded string.
        return encode(None);
    }

    let params = evm::GetStorageAtParams {
        storage_key: {
            let mut bz = [0u8; 32];
            position.to_big_endian(&mut bz);
            evm::uints::U256::from_big_endian(&bz)
        },
    };
    let params = RawBytes::serialize(params).context("failed to serialize position to IPLD")?;
    let ret = data
        .read_evm_actor::<evm::GetStorageAtReturn>(
            address,
            evm::Method::GetStorageAt,
            params,
            height,
        )
        .await?;

    if let Some(ret) = ret {
        // ret.storage.to_big_endian(&mut bz);
        return encode(Some(ret.storage));
    }

    encode(None)
}

/// Returns code at a given address.
pub async fn get_code<C>(
    data: JsonRpcData<C>,
    Params((address, block_id)): Params<(et::H160, et::BlockId)>,
) -> JsonRpcResult<et::Bytes>
where
    C: Client + Sync + Send,
{
    let height = data.query_height(block_id).await?;

    // Return empty if not an EVM actor.
    if data.get_actor_type(&address, height).await? != ActorType::EVM {
        return Ok(Default::default());
    }

    // This method has no input parameters.
    let params = RawBytes::default();

    let ret = data
        .read_evm_actor::<evm::BytecodeReturn>(address, evm::Method::GetBytecode, params, height)
        .await?;

    match ret.and_then(|r| r.code) {
        None => Ok(et::Bytes::default()),
        Some(cid) => {
            let code = data
                .client
                .ipld(&cid, height)
                .await
                .context("failed to fetch bytecode")?;

            Ok(code.map(et::Bytes::from).unwrap_or_default())
        }
    }
}

/// Returns an object with data about the sync status or false.
pub async fn syncing<C>(data: JsonRpcData<C>) -> JsonRpcResult<et::SyncingStatus>
where
    C: Client + Sync + Send,
{
    let status: status::Response = data.tm().status().await.context("failed to fetch status")?;
    let info = status.sync_info;
    let status = if !info.catching_up {
        et::SyncingStatus::IsFalse
    } else {
        let progress = et::SyncProgress {
            // This would be the block we executed.
            current_block: et::U64::from(info.latest_block_height.value()),
            // This would be the block we know about but haven't got to yet.
            highest_block: et::U64::from(info.latest_block_height.value()),
            // This would be the block we started syncing from.
            starting_block: Default::default(),
            pulled_states: None,
            known_states: None,
            healed_bytecode_bytes: None,
            healed_bytecodes: None,
            healed_trienode_bytes: None,
            healed_trienodes: None,
            healing_bytecode: None,
            healing_trienodes: None,
            synced_account_bytes: None,
            synced_accounts: None,
            synced_bytecode_bytes: None,
            synced_bytecodes: None,
            synced_storage: None,
            synced_storage_bytes: None,
        };
        et::SyncingStatus::IsSyncing(Box::new(progress))
    };

    Ok(status)
}

/// Returns an array of all logs matching a given filter object.
pub async fn get_logs<C>(
    data: JsonRpcData<C>,
    Params((filter,)): Params<(et::Filter,)>,
) -> JsonRpcResult<Vec<et::Log>>
where
    C: Client + Sync + Send,
{
    let (from_height, to_height) = match filter.block_option {
        et::FilterBlockOption::Range {
            from_block,
            to_block,
        } => {
            // Turn block number into a height.
            async fn resolve_height<C: Client + Send + Sync>(
                data: &JsonRpcData<C>,
                bn: BlockNumber,
            ) -> JsonRpcResult<Height> {
                match bn {
                    BlockNumber::Number(n) => {
                        Ok(Height::try_from(n.as_u64()).context("invalid height")?)
                    }
                    other => {
                        let h = data.header_by_height(other).await?;
                        Ok(h.height)
                    }
                }
            }

            let from_block = from_block.unwrap_or_default();
            let mut to_block = to_block.unwrap_or_default();

            // Automatically restrict the end to the highest available block to allow queries by fixed ranges.
            // This is only applied ot the end, not the start, so if `from > to` then we return nothing.
            if let BlockNumber::Number(n) = to_block {
                let latest_height = data.latest_height().await?;
                if n.as_u64() > latest_height.value() {
                    to_block = BlockNumber::Number(et::U64::from(latest_height.value()));
                }
            }

            // Resolve named heights to a number.
            let to_height = resolve_height(&data, to_block).await?;
            let from_height = if from_block == to_block {
                to_height
            } else {
                resolve_height(&data, from_block).await?
            };

            (from_height, to_height)
        }
        et::FilterBlockOption::AtBlockHash(block_hash) => {
            let header = data.header_by_hash(block_hash).await?;
            (header.height, header.height)
        }
    };

    let addrs = match &filter.address {
        Some(et::ValueOrArray::Value(addr)) => vec![*addr],
        Some(et::ValueOrArray::Array(addrs)) => addrs.clone(),
        None => Vec::new(),
    };
    let addrs = addrs
        .into_iter()
        .map(|addr| Address::from(EthAddress(addr.0)))
        .collect::<HashSet<_>>();

    let mut height = from_height;
    let mut logs = Vec::new();

    while height <= to_height {
        if let Ok(block_results) = data.tm().block_results(height).await {
            if let Some(tx_results) = block_results.txs_results {
                let block_number = et::U64::from(height.value());

                let block = data
                    .block_by_height(et::BlockNumber::Number(block_number))
                    .await?;

                let block_hash = et::H256::from_slice(block.header().hash().as_bytes());

                let mut log_index_start = 0usize;
                for ((tx_idx, tx_result), tx) in tx_results.iter().enumerate().zip(block.data()) {
                    let msg = match to_chain_message(tx) {
                        Ok(ChainMessage::Signed(msg)) => msg,
                        _ => continue,
                    };

                    let emitters = from_tm::collect_emitters(&tx_result.events);

                    // Filter by address.
                    if !addrs.is_empty()
                        && !addrs.contains(&msg.message().from)
                        && !addrs.contains(&msg.message().to)
                        && addrs.intersection(&emitters).next().is_none()
                    {
                        continue;
                    }

                    let tx_hash = msg_hash(&tx_result.events, tx);
                    let tx_idx = et::U64::from(tx_idx);

                    let mut tx_logs = from_tm::to_logs(
                        &tx_result.events,
                        block_hash,
                        block_number,
                        tx_hash,
                        tx_idx,
                        log_index_start,
                    )?;

                    // Filter by topic.
                    tx_logs.retain(|log| matches_topics(&filter, log));

                    logs.append(&mut tx_logs);

                    log_index_start += tx_result.events.len();
                }
            }
        } else {
            break;
        }
        height = height.increment()
    }

    Ok(logs)
}

/// Creates a filter object, based on filter options, to notify when the state changes (logs).
/// To check if the state has changed, call eth_getFilterChanges.
pub async fn new_filter<C>(
    data: JsonRpcData<C>,
    Params((filter,)): Params<(et::Filter,)>,
) -> JsonRpcResult<FilterId>
where
    C: Client + SubscriptionClient + Clone + Sync + Send + 'static,
{
    let id = data
        .new_filter(FilterKind::Logs(Box::new(filter)))
        .await
        .context("failed to add log filter")?;
    Ok(id)
}

/// Creates a filter in the node, to notify when a new block arrives.
/// To check if the state has changed, call eth_getFilterChanges.
pub async fn new_block_filter<C>(data: JsonRpcData<C>) -> JsonRpcResult<FilterId>
where
    C: Client + SubscriptionClient + Clone + Sync + Send + 'static,
{
    let id = data
        .new_filter(FilterKind::NewBlocks)
        .await
        .context("failed to add block filter")?;
    Ok(id)
}

/// Creates a filter in the node, to notify when new pending transactions arrive.
/// To check if the state has changed, call eth_getFilterChanges.
pub async fn new_pending_transaction_filter<C>(data: JsonRpcData<C>) -> JsonRpcResult<FilterId>
where
    C: Client + SubscriptionClient + Clone + Sync + Send + 'static,
{
    let id = data
        .new_filter(FilterKind::PendingTransactions)
        .await
        .context("failed to add transaction filter")?;
    Ok(id)
}

/// Uninstalls a filter with given id. Should always be called when watch is no longer needed.
/// Additionally Filters timeout when they aren't requested with eth_getFilterChanges for a period of time
pub async fn uninstall_filter<C>(
    data: JsonRpcData<C>,
    Params((filter_id,)): Params<(FilterId,)>,
) -> JsonRpcResult<bool> {
    Ok(data.uninstall_filter(filter_id).await?)
}

pub async fn get_filter_changes<C>(
    data: JsonRpcData<C>,
    Params((filter_id,)): Params<(FilterId,)>,
) -> JsonRpcResult<Vec<serde_json::Value>> {
    if let Some(records) = data.take_filter_changes(filter_id).await? {
        let records = records
            .to_json_vec()
            .context("failed to convert filter changes")?;
        Ok(records)
    } else {
        error(ExitCode::USR_NOT_FOUND, "filter not found")
    }
}

/// Returns an array of all logs matching filter with given id.
pub async fn get_filter_logs<C>(
    data: JsonRpcData<C>,
    Params((filter_id,)): Params<(FilterId,)>,
) -> JsonRpcResult<Vec<et::Log>> {
    if let Some(accum) = data.take_filter_changes(filter_id).await? {
        match accum {
            FilterRecords::Logs(logs) => Ok(logs),
            FilterRecords::NewBlocks(_) | FilterRecords::PendingTransactions(_) => {
                error(ExitCode::USR_ILLEGAL_STATE, "not a log filter")
            }
        }
    } else {
        error(ExitCode::USR_NOT_FOUND, "filter not found")
    }
}

/// Subscribe to a filter and send the data to a websocket.
pub async fn subscribe<C>(
    data: JsonRpcData<C>,
    Params(params): Params<SubscribeParams>,
) -> JsonRpcResult<FilterId>
where
    C: Client + SubscriptionClient + Clone + Sync + Send + 'static,
{
    match params {
        SubscribeParams::One((tag, web_socket_id)) => match tag.as_str() {
            "newHeads" => {
                // Subscribe to `Block<TxHash>`
                let ws_sender = data.get_web_socket(&web_socket_id).await?;
                let id = data
                    .new_subscription(FilterKind::NewBlocks, ws_sender)
                    .await
                    .context("failed to add block subscription")?;
                Ok(id)
            }
            "newPendingTransactions" => {
                // Subscribe to `TxHash`
                let ws_sender = data.get_web_socket(&web_socket_id).await?;
                let id = data
                    .new_subscription(FilterKind::PendingTransactions, ws_sender)
                    .await
                    .context("failed to add transaction subscription")?;
                Ok(id)
            }
            other => error(
                ExitCode::USR_ILLEGAL_ARGUMENT,
                format!("unknown subscription: {other}"),
            ),
        },
        SubscribeParams::Two((tag, filter, web_socket_id)) => match tag.as_str() {
            "logs" => {
                // Subscribe to `Log`
                let ws_sender = data.get_web_socket(&web_socket_id).await?;
                let id = data
                    .new_subscription(FilterKind::Logs(Box::new(filter)), ws_sender)
                    .await
                    .context("failed to add transaction subscription")?;
                Ok(id)
            }
            other => error(
                ExitCode::USR_ILLEGAL_ARGUMENT,
                format!("unknown subscription: {other}"),
            ),
        },
    }
}

/// Unsubscribe from the filter registered by this websocket.
pub async fn unsubscribe<C>(
    data: JsonRpcData<C>,
    Params((filter_id,)): Params<(FilterId,)>,
) -> JsonRpcResult<bool> {
    uninstall_filter(data, Params((filter_id,))).await
}

use crate::state::ActorType;
use params::{EstimateGasParams, SubscribeParams, TypedTransactionCompat};

mod params {
    use ethers_core::types::transaction::eip2718::TypedTransaction;
    use ethers_core::types::Eip1559TransactionRequest;
    use ethers_core::types::{self as et, Eip2930TransactionRequest, TransactionRequest};
    use serde::Deserialize;

    use crate::state::WebSocketId;

    /// Copied from `ethers` to override `data` deserialization.
    ///
    /// See <https://github.com/filecoin-project/lotus/pull/11471>
    ///
    /// This is accepted for gas estimation only.
    #[derive(Clone, Default, Deserialize, PartialEq, Eq, Debug)]
    pub struct TransactionRequestCompat {
        #[serde(flatten)]
        orig: TransactionRequest,
        input: Option<et::Bytes>,
    }

    impl From<TransactionRequestCompat> for TransactionRequest {
        fn from(value: TransactionRequestCompat) -> Self {
            let mut request = value.orig;
            request.data = value.input.or(request.data);
            request
        }
    }

    /// Copied from `ethers` to override `data` deserialization.
    ///
    /// See <https://github.com/filecoin-project/lotus/pull/11471>
    #[derive(Clone, Default, Deserialize, PartialEq, Eq, Debug)]
    pub struct Eip1559TransactionRequestCompat {
        #[serde(flatten)]
        orig: Eip1559TransactionRequest,
        input: Option<et::Bytes>,
    }

    impl From<Eip1559TransactionRequestCompat> for Eip1559TransactionRequest {
        fn from(value: Eip1559TransactionRequestCompat) -> Self {
            let mut request = value.orig;
            request.data = value.input.or(request.data);
            request
        }
    }

    #[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
    // NOTE: Using untagged so is able to deserialize as a legacy transaction
    // directly if the type is not set. Needed for backward compatibility.
    // #[serde(tag = "type")]
    #[serde(untagged)]
    pub enum TypedTransactionCompat {
        #[serde(rename = "0x00", alias = "0x0")]
        Legacy(TransactionRequestCompat),
        #[serde(rename = "0x02", alias = "0x2")]
        Eip1559(Eip1559TransactionRequestCompat),
        #[serde(rename = "0x01", alias = "0x1")]
        Eip2930(Eip2930TransactionRequest),
    }

    impl From<TypedTransactionCompat> for TypedTransaction {
        fn from(value: TypedTransactionCompat) -> Self {
            match value {
                TypedTransactionCompat::Eip1559(v) => TypedTransaction::Eip1559(v.into()),
                TypedTransactionCompat::Legacy(v) => TypedTransaction::Legacy(v.into()),
                TypedTransactionCompat::Eip2930(v) => TypedTransaction::Eip2930(v),
            }
        }
    }

    /// The client either sends one or two items in the array, depending on whether a block ID is specified.
    /// This is to keep it backwards compatible with nodes that do not support the block ID parameter.
    /// If we were using `Option`, they would have to send `null`; this way it works with both 1 or 2 parameters.
    #[derive(Deserialize)]
    #[serde(untagged)]
    pub enum EstimateGasParams {
        One((TypedTransactionCompat,)),
        Two((TypedTransactionCompat, et::BlockId)),
    }

    /// The client either sends one or two items in the array, depending on whether it's subscribing to block,
    /// transactions or logs. To that we add the web socket ID.
    #[derive(Deserialize)]
    #[serde(untagged)]
    #[allow(clippy::large_enum_variant)]
    pub enum SubscribeParams {
        One((String, WebSocketId)),
        Two((String, et::Filter, WebSocketId)),
    }

    #[cfg(test)]
    mod tests {
        use ethers_core::types::Eip1559TransactionRequest;

        use crate::apis::eth::params::{Eip1559TransactionRequestCompat, EstimateGasParams};

        #[test]
        fn deserialize_estimate_gas_params() {
            let raw_str = r#"
            [{"data":"0x6080806040523461001a576101949081610020823930815050f35b600080fdfe608080604052600436101561001357600080fd5b600090813560e01c90816325ca4c9c146100715750635d3f8a691461003757600080fd5b602036600319011261006e576004356001600160a01b0381169081900361006a5760405160ff60981b9091148152602090f35b5080fd5b80fd5b9050602036600319011261006a576004356001600160a01b038116810361015a57803b15918261012f575b826100af575b6020836040519015158152f35b908092503b67ffffffffffffffff80821161011b57601f8201601f19908116603f011683019081118382101761011b579360209460405281835284830180943c5190207fc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a4701438806100a2565b634e487b7160e01b85526041600452602485fd5b7fc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470823f14925061009c565b8280fdfea264697066735822122001bdefe53a9918e1f0e577b34ea5aed929e1b2cb9d7dd151f3a2d35024d5616f64736f6c63430008130033","from":"0x1a79385ead0e873fe0c441c034636d3edf7014cc","maxFeePerGas":"0x596836d0","maxPriorityFeePerGas":"0x59682f00","type":"0x2"}]
            "#;
            let r = serde_json::from_str::<EstimateGasParams>(raw_str);
            assert!(r.is_ok());
        }

        #[test]
        fn deserialize_input_and_data() {
            let examples = [
                ("01", r#" "data":"0x0d", "input": "0x01" "#),
                ("02", r#" "data":"0x02" "#),
                ("03", r#" "input":"0x03" "#),
            ];
            for (exp, frag) in examples {
                let json = format!(
                    "{{ {frag}, \"from\":\"0x1a79385ead0e873fe0c441c034636d3edf7014cc\",\"maxFeePerGas\":\"0x596836d0\",\"maxPriorityFeePerGas\":\"0x59682f00\" }}"
                );

                let r: Eip1559TransactionRequest =
                    serde_json::from_str::<Eip1559TransactionRequestCompat>(&json)
                        .unwrap_or_else(|e| panic!("failed to parse {json}: {e}"))
                        .into();

                let d = r.data.expect("data is empty");

                assert_eq!(hex::encode(d), exp)
            }
        }
    }
}
