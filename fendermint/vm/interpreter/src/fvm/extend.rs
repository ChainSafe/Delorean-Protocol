// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use async_trait::async_trait;
use fendermint_actor_cetf::Tag;
use std::collections::HashMap;

use fendermint_vm_actor_interface::{cetf, chainmetadata, cron, system};
use fvm::executor::ApplyRet;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::{address::Address, ActorID, MethodNum, BLOCK_GAS_LIMIT};
use tendermint_rpc::Client;

use crate::ExtendVoteInterpreter;

use super::{
    checkpoint::{self, PowerUpdates},
    state::FvmExecState,
    FvmMessage, FvmMessageInterpreter,
};

#[async_trait]
impl<DB, TC> ExtendVoteInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + Clone + 'static + Send + Sync,
    TC: Client + Clone + Send + Sync + 'static,
{
    type State = FvmExecState<DB>;
    type Message = Tag;

    type Output = Option<bls_signatures::Signature>;

    /// Sign the vote.
    fn extend_vote(&self, msg: Self::Message) -> anyhow::Result<Self::Output> {
        if let Some(ctx) = self.validator_ctx.as_ref() {
            let sig = ctx.bls_secret_key.sign(&msg);
            Ok(Some(sig))
        } else {
            Ok(None)
        }
    }

    async fn verify_vote_extension(
        &self,
        state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        todo!("Unimplemented")
    }
}
