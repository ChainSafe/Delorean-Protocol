// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use async_trait::async_trait;
use cid::Cid;
use fendermint_vm_message::query::{ActorState, FvmQuery, GasEstimate, StateParams};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{
    bigint::BigInt, econ::TokenAmount, error::ExitCode, message::Message, ActorID, BLOCK_GAS_LIMIT,
};
use num_traits::Zero;

use crate::QueryInterpreter;

use super::{state::FvmQueryState, FvmApplyRet, FvmMessageInterpreter};

/// Internal return type for queries. It will never be serialized
/// and sent over the wire as it is, only its internal parts are
/// sent in the response. The client has to know what to expect,
/// depending on the kind of query it sent.
pub enum FvmQueryRet {
    /// Bytes from the IPLD store result, if found.
    Ipld(Option<Vec<u8>>),
    /// The full state of an actor, if found.
    ActorState(Option<Box<(ActorID, ActorState)>>),
    /// The results of a read-only message application.
    Call(FvmApplyRet),
    /// The estimated gas limit.
    EstimateGas(GasEstimate),
    /// Current state parameters.
    StateParams(StateParams),
    /// Builtin actors known by the system.
    BuiltinActors(Vec<(String, Cid)>),
}

#[async_trait]
impl<DB, TC> QueryInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + 'static + Send + Sync + Clone,
    TC: Send + Sync + 'static,
{
    type State = FvmQueryState<DB>;
    type Query = FvmQuery;
    type Output = FvmQueryRet;

    async fn query(
        &self,
        state: Self::State,
        qry: Self::Query,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        match qry {
            FvmQuery::Ipld(cid) => {
                let data = state.store_get(&cid)?;
                tracing::info!(
                    height = state.block_height(),
                    pending = state.pending(),
                    cid = cid.to_string(),
                    found = data.is_some(),
                    "query IPLD"
                );
                let out = FvmQueryRet::Ipld(data);
                Ok((state, out))
            }
            FvmQuery::ActorState(address) => {
                let (state, ret) = state.actor_state(&address).await?;
                tracing::info!(
                    height = state.block_height(),
                    pending = state.pending(),
                    addr = address.to_string(),
                    found = ret.is_some(),
                    "query actor state"
                );
                let out = FvmQueryRet::ActorState(ret.map(Box::new));
                Ok((state, out))
            }
            FvmQuery::Call(msg) => {
                let from = msg.from;
                let to = msg.to;
                let method_num = msg.method_num;
                let gas_limit = msg.gas_limit;

                // Do not stack effects
                let (state, (apply_ret, emitters)) = state.call(*msg).await?;

                tracing::info!(
                    height = state.block_height(),
                    pending = state.pending(),
                    to = to.to_string(),
                    from = from.to_string(),
                    method_num,
                    exit_code = apply_ret.msg_receipt.exit_code.value(),
                    data = hex::encode(apply_ret.msg_receipt.return_data.bytes()),
                    info = apply_ret
                        .failure_info
                        .as_ref()
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    "query call"
                );

                let ret = FvmApplyRet {
                    apply_ret,
                    from,
                    to,
                    method_num,
                    gas_limit,
                    emitters,
                };

                let out = FvmQueryRet::Call(ret);
                Ok((state, out))
            }
            FvmQuery::EstimateGas(mut msg) => {
                tracing::info!(
                    height = state.block_height(),
                    pending = state.pending(),
                    to = msg.to.to_string(),
                    from = msg.from.to_string(),
                    method_num = msg.method_num,
                    "query estimate gas"
                );
                // Populate gas message parameters.

                match self.estimate_gassed_msg(state, &mut msg).await? {
                    (state, Some(est)) => {
                        // return immediately if something is returned,
                        // it means that the message failed to execute so there's
                        // no point on estimating the gas.
                        Ok((state, FvmQueryRet::EstimateGas(est)))
                    }
                    (state, None) => {
                        // perform a gas search for an accurate value
                        let (state, mut est) = self.gas_search(state, &msg).await?;
                        // we need an additional overestimation for the case where
                        // the exact value is returned as part of the gas search
                        // (for some reason with subsequent calls sometimes this is the case).
                        est.gas_limit =
                            (est.gas_limit as f64 * self.gas_overestimation_rate) as u64;

                        Ok((state, FvmQueryRet::EstimateGas(est)))
                    }
                }
            }
            FvmQuery::StateParams => {
                let state_params = state.state_params();
                let state_params = StateParams {
                    base_fee: state_params.base_fee.clone(),
                    circ_supply: state_params.circ_supply.clone(),
                    chain_id: state_params.chain_id,
                    network_version: state_params.network_version,
                };
                Ok((state, FvmQueryRet::StateParams(state_params)))
            }
            FvmQuery::BuiltinActors => {
                let (state, ret) = state.builtin_actors().await?;
                Ok((state, FvmQueryRet::BuiltinActors(ret)))
            }
        }
    }
}

impl<DB, TC> FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + 'static + Send + Sync + Clone,
{
    async fn estimate_gassed_msg(
        &self,
        state: FvmQueryState<DB>,
        msg: &mut Message,
    ) -> anyhow::Result<(FvmQueryState<DB>, Option<GasEstimate>)> {
        // Setting BlockGasLimit as initial limit for gas estimation
        msg.gas_limit = BLOCK_GAS_LIMIT;

        // With unlimited gas we are probably better off setting the prices to zero.
        let gas_premium = msg.gas_premium.clone();
        let gas_fee_cap = msg.gas_fee_cap.clone();
        msg.gas_premium = TokenAmount::zero();
        msg.gas_fee_cap = TokenAmount::zero();

        // estimate the gas limit and assign it to the message
        // revert any changes because we'll repeat the estimation
        let (state, (ret, _)) = state.call(msg.clone()).await?;

        tracing::debug!(
            gas_used = ret.msg_receipt.gas_used,
            exit_code = ret.msg_receipt.exit_code.value(),
            "estimated gassed message"
        );

        if !ret.msg_receipt.exit_code.is_success() {
            // if the message fail we can't estimate the gas.
            return Ok((
                state,
                Some(GasEstimate {
                    exit_code: ret.msg_receipt.exit_code,
                    info: ret.failure_info.map(|x| x.to_string()).unwrap_or_default(),
                    return_data: ret.msg_receipt.return_data,
                    gas_limit: 0,
                }),
            ));
        }

        msg.gas_limit = (ret.msg_receipt.gas_used as f64 * self.gas_overestimation_rate) as u64;

        if gas_premium.is_zero() {
            // We need to set the gas_premium to some value other than zero for the
            // gas estimation to work accurately (I really don't know why this is
            // the case but after a lot of testing, setting this value to zero rejects the transaction)
            msg.gas_premium = TokenAmount::from_nano(BigInt::from(1));
        } else {
            msg.gas_premium = gas_premium;
        }

        // Same for the gas_fee_cap, not setting the fee cap leads to the message
        // being sent after the estimation to fail.
        if gas_fee_cap.is_zero() {
            // TODO: In Lotus historical values of the base fee and a more accurate overestimation is performed
            // for the fee cap. If we issues with messages going through let's consider the historical analysis.
            // For now we are disregarding the base_fee so I don't think this is needed here.
            // Filecoin clamps the gas premium at GasFeeCap - BaseFee, if lower than the
            // specified premium. Returns 0 if GasFeeCap is less than BaseFee.
            // see https://spec.filecoin.io/#section-systems.filecoin_vm.message.message-semantic-validation
            msg.gas_fee_cap = msg.gas_premium.clone();
        } else {
            msg.gas_fee_cap = gas_fee_cap;
        }

        Ok((state, None))
    }

    // This function performs a simpler implementation of the gas search than the one used in Lotus.
    // Instead of using historical information of the gas limit for other messages, it searches
    // for a valid gas limit for the current message in isolation.
    async fn gas_search(
        &self,
        mut state: FvmQueryState<DB>,
        msg: &Message,
    ) -> anyhow::Result<(FvmQueryState<DB>, GasEstimate)> {
        let mut curr_limit = msg.gas_limit;

        loop {
            let (st, est) = self
                .estimation_call_with_limit(state, msg.clone(), curr_limit)
                .await?;

            if let Some(est) = est {
                return Ok((st, est));
            } else {
                state = st;
            }

            curr_limit = (curr_limit as f64 * self.gas_search_step) as u64;
            if curr_limit > BLOCK_GAS_LIMIT {
                let est = GasEstimate {
                    exit_code: ExitCode::OK,
                    info: "".to_string(),
                    return_data: RawBytes::default(),
                    gas_limit: BLOCK_GAS_LIMIT,
                };
                return Ok((state, est));
            }
        }

        // TODO: For a more accurate gas estimation we could track the low and the high
        // of the search and make higher steps (e.g. `GAS_SEARCH_STEP = 2`).
        // Once an interval is found of [low, high] for which the message
        // succeeds, we make a finer-grained within that interval.
        // At this point, I don't think is worth being that accurate as long as it works.
    }

    async fn estimation_call_with_limit(
        &self,
        state: FvmQueryState<DB>,
        mut msg: Message,
        limit: u64,
    ) -> anyhow::Result<(FvmQueryState<DB>, Option<GasEstimate>)> {
        msg.gas_limit = limit;
        // set message nonce to zero so the right one is picked up
        msg.sequence = 0;

        let (state, (apply_ret, _)) = state.call(msg).await?;

        let ret = GasEstimate {
            exit_code: apply_ret.msg_receipt.exit_code,
            info: apply_ret
                .failure_info
                .map(|x| x.to_string())
                .unwrap_or_default(),
            return_data: apply_ret.msg_receipt.return_data,
            gas_limit: apply_ret.msg_receipt.gas_used,
        };

        // if the message succeeded or failed with a different error than `SYS_OUT_OF_GAS`,
        // immediately return as we either succeeded finding the right gas estimation,
        // or something non-related happened.
        if ret.exit_code == ExitCode::OK || ret.exit_code != ExitCode::SYS_OUT_OF_GAS {
            return Ok((state, Some(ret)));
        }

        Ok((state, None))
    }
}
