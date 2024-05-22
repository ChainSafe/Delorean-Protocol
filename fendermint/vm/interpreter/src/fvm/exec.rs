// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use async_trait::async_trait;
use std::collections::HashMap;

use fendermint_vm_actor_interface::{chainmetadata, cron, system, cetf};
use fvm::executor::ApplyRet;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{address::Address, ActorID, MethodNum, BLOCK_GAS_LIMIT};
use tendermint_rpc::Client;

use crate::ExecInterpreter;

use super::{
    checkpoint::{self, PowerUpdates},
    state::FvmExecState,
    FvmMessage, FvmMessageInterpreter,
};

/// The return value extended with some things from the message that
/// might not be available to the caller, because of the message lookups
/// and transformations that happen along the way, e.g. where we need
/// a field, we might just have a CID.
pub struct FvmApplyRet {
    pub apply_ret: ApplyRet,
    pub from: Address,
    pub to: Address,
    pub method_num: MethodNum,
    pub gas_limit: u64,
    /// Delegated addresses of event emitters, if they have one.
    pub emitters: HashMap<ActorID, Address>,
}

#[async_trait]
impl<DB, TC> ExecInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + Clone + 'static + Send + Sync,
    TC: Client + Clone + Send + Sync + 'static,
{
    type State = FvmExecState<DB>;
    type Message = FvmMessage;
    type BeginOutput = FvmApplyRet;
    type DeliverOutput = FvmApplyRet;
    /// Return validator power updates.
    /// Currently ignoring events as there aren't any emitted by the smart contract,
    /// but keep in mind that if there were, those would have to be propagated.
    type EndOutput = PowerUpdates;

    async fn begin(
        &self,
        mut state: Self::State,
    ) -> anyhow::Result<(Self::State, Self::BeginOutput)> {
        // Block height (FVM epoch) as sequence is intentional
        let height = state.block_height();

        // check for upgrades in the upgrade_scheduler
        let chain_id = state.chain_id();
        let block_height: u64 = state.block_height().try_into().unwrap();
        if let Some(upgrade) = self.upgrade_scheduler.get(chain_id, block_height) {
            // TODO: consider using an explicit tracing enum for upgrades
            tracing::info!(?chain_id, height = block_height, "Executing an upgrade");

            // there is an upgrade scheduled for this height, lets run the migration
            let res = upgrade.execute(&mut state).context("upgrade failed")?;
            if let Some(new_app_version) = res {
                state.update_app_version(|app_version| {
                    *app_version = new_app_version;
                });

                tracing::info!(app_version = state.app_version(), "upgraded app version");
            }
        }

        // Arbitrarily large gas limit for cron (matching how Forest does it, which matches Lotus).
        // XXX: Our blocks are not necessarily expected to be 30 seconds apart, so the gas limit might be wrong.
        let gas_limit = BLOCK_GAS_LIMIT * 10000;
        let from = system::SYSTEM_ACTOR_ADDR;
        let to = cron::CRON_ACTOR_ADDR;
        let method_num = cron::Method::EpochTick as u64;

        // Cron.
        let msg = FvmMessage {
            from,
            to,
            sequence: height as u64,
            gas_limit,
            method_num,
            params: Default::default(),
            value: Default::default(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        };

        let (apply_ret, emitters) = state.execute_implicit(msg)?;

        // Failing cron would be fatal.
        if let Some(err) = apply_ret.failure_info {
            anyhow::bail!("failed to apply block cron message: {}", err);
        }

        // Push the current block hash to the chainmetadata actor
        if self.push_chain_meta {
            if let Some(block_hash) = state.block_hash() {
                let params = fvm_ipld_encoding::RawBytes::serialize(
                    fendermint_actor_chainmetadata::PushBlockParams {
                        epoch: height,
                        block: block_hash,
                    },
                )?;

                let msg = FvmMessage {
                    from: system::SYSTEM_ACTOR_ADDR,
                    to: chainmetadata::CHAINMETADATA_ACTOR_ADDR,
                    sequence: height as u64,
                    gas_limit,
                    method_num: fendermint_actor_chainmetadata::Method::PushBlockHash as u64,
                    params,
                    value: Default::default(),
                    version: Default::default(),
                    gas_fee_cap: Default::default(),
                    gas_premium: Default::default(),
                };

                let (apply_ret, _) = state.execute_implicit(msg)?;

                if let Some(err) = apply_ret.failure_info {
                    anyhow::bail!("failed to apply chainmetadata message: {}", err);
                }
            }
        }

        // Invoke some actor functionality on each new block
        {
            let params = fvm_ipld_encoding::RawBytes::serialize(
                fendermint_actor_cetf::EnqueueTagParams {
                    tag: [0xfd; 32],
                },
            )?;
            let msg = FvmMessage {
                from: system::SYSTEM_ACTOR_ADDR,
                to: cetf::CETFSYSCALL_ACTOR_ADDR,
                sequence: height as u64,
                gas_limit,
                method_num: fendermint_actor_cetf::Method::EnqueueTag as u64,
                params,
                value: Default::default(),
                version: Default::default(),
                gas_fee_cap: Default::default(),
                gas_premium: Default::default(),
            };

            let (apply_ret, _) = state.execute_implicit(msg)?;

            if let Some(err) = apply_ret.failure_info {
                anyhow::bail!("failed to apply cetf message: {}", err);
            }
        }

        let ret = FvmApplyRet {
            apply_ret,
            from,
            to,
            method_num,
            gas_limit,
            emitters,
        };

        Ok((state, ret))
    }

    async fn deliver(
        &self,
        mut state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<(Self::State, Self::DeliverOutput)> {
        let from = msg.from;
        let to = msg.to;
        let method_num = msg.method_num;
        let gas_limit = msg.gas_limit;

        let (apply_ret, emitters) = if from == system::SYSTEM_ACTOR_ADDR {
            state.execute_implicit(msg)?
        } else {
            state.execute_explicit(msg)?
        };

        tracing::info!(
            height = state.block_height(),
            from = from.to_string(),
            to = to.to_string(),
            method_num = method_num,
            exit_code = apply_ret.msg_receipt.exit_code.value(),
            gas_used = apply_ret.msg_receipt.gas_used,
            "tx delivered"
        );

        let ret = FvmApplyRet {
            apply_ret,
            from,
            to,
            method_num,
            gas_limit,
            emitters,
        };

        Ok((state, ret))
    }

    async fn end(&self, mut state: Self::State) -> anyhow::Result<(Self::State, Self::EndOutput)> {
        let updates = if let Some((checkpoint, updates)) =
            checkpoint::maybe_create_checkpoint(&self.gateway, &mut state)
                .context("failed to create checkpoint")?
        {
            // Asynchronously broadcast signature, if validating.
            if let Some(ref ctx) = self.validator_ctx {
                // Do not resend past signatures.
                if !self.syncing().await {
                    // Fetch any incomplete checkpoints synchronously because the state can't be shared across threads.
                    let incomplete_checkpoints =
                        checkpoint::unsigned_checkpoints(&self.gateway, &mut state, ctx.public_key)
                            .context("failed to fetch incomplete checkpoints")?;

                    debug_assert!(
                        incomplete_checkpoints
                            .iter()
                            .any(|cp| cp.block_height == checkpoint.block_height
                                && cp.block_hash == checkpoint.block_hash),
                        "the current checkpoint is incomplete"
                    );

                    let client = self.client.clone();
                    let gateway = self.gateway.clone();
                    let chain_id = state.chain_id();
                    let height = checkpoint.block_height;
                    let validator_ctx = ctx.clone();

                    tokio::spawn(async move {
                        let res = checkpoint::broadcast_incomplete_signatures(
                            &client,
                            &validator_ctx,
                            &gateway,
                            chain_id,
                            incomplete_checkpoints,
                        )
                        .await;

                        if let Err(e) = res {
                            tracing::error!(error =? e, height = height.as_u64(), "error broadcasting checkpoint signature");
                        }
                    });
                }
            }

            updates
        } else {
            PowerUpdates::default()
        };

        Ok((state, updates))
    }
}
