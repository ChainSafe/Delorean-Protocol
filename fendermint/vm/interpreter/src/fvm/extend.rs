// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use fendermint_actor_cetf::Tag;

use fvm_ipld_blockstore::Blockstore;
use tendermint_rpc::Client;

use crate::ExtendVoteInterpreter;

use super::{
    state::{FvmExecState, FvmQueryState},
    FvmMessageInterpreter,
};

pub enum ExtendVoteKind {
    Tag(Tag),
    None,
}
#[async_trait]
impl<DB, TC> ExtendVoteInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + Clone + 'static + Send + Sync,
    TC: Client + Clone + Send + Sync + 'static,
{
    type State = FvmQueryState<DB>;
    type Message = ExtendVoteKind;

    type Output = Option<bls_signatures::Signature>;

    /// Sign the vote.
    fn extend_vote(&self, _state: Self::State, msg: Self::Message) -> anyhow::Result<Self::Output> {
        match msg {
            ExtendVoteKind::Tag(tag) => {
                if let Some(ctx) = self.validator_ctx.as_ref() {
                    let sig = ctx.bls_secret_key.sign(&tag);
                    Ok(Some(sig))
                } else {
                    Ok(None)
                }
            }
            ExtendVoteKind::None => Ok(None),
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
