// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cid::Cid;
use fendermint_vm_genesis::Genesis;
use fendermint_vm_message::chain::ChainMessage;
use fvm_ipld_encoding::Error as IpldError;

use crate::{
    chain::{ChainMessageApplyRet, ChainMessageCheckRes},
    fvm::{FvmQuery, FvmQueryRet},
    CheckInterpreter, ExecInterpreter, ExtendVoteInterpreter, GenesisInterpreter,
    ProposalInterpreter, QueryInterpreter,
};

pub type BytesMessageApplyRes = Result<ChainMessageApplyRet, IpldError>;
pub type BytesMessageCheckRes = Result<ChainMessageCheckRes, IpldError>;
pub type BytesMessageQueryRes = Result<FvmQueryRet, IpldError>;

/// Close to what the ABCI sends: (Path, Bytes).
pub type BytesMessageQuery = (String, Vec<u8>);

/// Behavour of proposal preparation. It's an optimisation to cut down needless serialization
/// when we know we aren't doing anything with the messages.
#[derive(Debug, Default, Clone)]
pub enum ProposalPrepareMode {
    /// Deserialize all messages and pass them to the inner interpreter.
    #[default]
    PassThrough,
    /// Does not pass messages to the inner interpreter, only appends what is returned from it.
    AppendOnly,
    /// Does not pass messages to the inner interpreter, only prepends what is returned from it.
    PrependOnly,
}

/// Interpreter working on raw bytes.
#[derive(Clone)]
pub struct BytesMessageInterpreter<I> {
    inner: I,
    /// Should we parse and pass on all messages during prepare.
    prepare_mode: ProposalPrepareMode,
    /// Should we reject proposals with transactions we cannot parse.
    reject_malformed_proposal: bool,
    /// Maximum number of messages to allow in a block.
    max_msgs: usize,
}

impl<I> BytesMessageInterpreter<I> {
    pub fn new(
        inner: I,
        prepare_mode: ProposalPrepareMode,
        reject_malformed_proposal: bool,
        max_msgs: usize,
    ) -> Self {
        Self {
            inner,
            prepare_mode,
            reject_malformed_proposal,
            max_msgs,
        }
    }
}

#[async_trait]
impl<I> ProposalInterpreter for BytesMessageInterpreter<I>
where
    I: ProposalInterpreter<Message = ChainMessage>,
{
    type State = I::State;
    type Message = Vec<u8>;

    /// Parse messages in the mempool and pass them into the inner `ChainMessage` interpreter.
    async fn prepare(
        &self,
        state: Self::State,
        msgs: Vec<Self::Message>,
    ) -> anyhow::Result<Vec<Self::Message>> {
        // Collect the messages to pass to the inner interpreter.
        let chain_msgs = match self.prepare_mode {
            ProposalPrepareMode::PassThrough => {
                let mut chain_msgs = Vec::new();
                for msg in msgs.iter() {
                    match fvm_ipld_encoding::from_slice::<ChainMessage>(msg) {
                        Err(e) => {
                            // This should not happen because the `CheckInterpreter` implementation below would
                            // have rejected any such user transaction.
                            tracing::warn!(
                                error = e.to_string(),
                                "failed to decode message in mempool as ChainMessage"
                            );
                        }
                        Ok(msg) => chain_msgs.push(msg),
                    }
                }
                chain_msgs
            }
            ProposalPrepareMode::AppendOnly | ProposalPrepareMode::PrependOnly => Vec::new(),
        };

        let chain_msgs = self.inner.prepare(state, chain_msgs).await?;

        let chain_msgs = chain_msgs
            .into_iter()
            .map(|msg| {
                fvm_ipld_encoding::to_vec(&msg).context("failed to encode ChainMessage as IPLD")
            })
            .collect::<anyhow::Result<Vec<Self::Message>>>()?;

        let mut all_msgs = match self.prepare_mode {
            ProposalPrepareMode::PassThrough => chain_msgs,
            ProposalPrepareMode::AppendOnly => [msgs, chain_msgs].concat(),
            ProposalPrepareMode::PrependOnly => [chain_msgs, msgs].concat(),
        };

        if all_msgs.len() > self.max_msgs {
            tracing::warn!(
                max_msgs = self.max_msgs,
                all_msgs = all_msgs.len(),
                "truncating proposal"
            );
            all_msgs.truncate(self.max_msgs);
        }

        Ok(all_msgs)
    }

    /// Parse messages in the block, reject if unknown format. Pass the rest to the inner `ChainMessage` interpreter.
    async fn process(&self, state: Self::State, msgs: Vec<Self::Message>) -> anyhow::Result<bool> {
        if msgs.len() > self.max_msgs {
            tracing::warn!(
                block_msgs = msgs.len(),
                "rejecting block: too many messages"
            );
            return Ok(false);
        }

        let mut chain_msgs = Vec::new();
        for msg in msgs {
            match fvm_ipld_encoding::from_slice::<ChainMessage>(&msg) {
                Err(e) => {
                    // If we cannot parse a message, then either:
                    // * The proposer is Byzantine - as an attack this isn't very effective as they could just not send a proposal and cause a timeout.
                    // * Our or the proposer node have different versions, or contain bugs
                    // We can either vote for it or not:
                    // * If we accept, we can punish the validator during block execution, and if it turns out we had a bug, we will have a consensus failure.
                    // * If we accept, then the serialization error will become visible in the transaction results through RPC.
                    // * If we reject, the majority can still accept the block, which indicates we had the bug (that way we might even panic during delivery, since we know it got voted on),
                    //   but a buggy transaction format that fails for everyone would cause liveness issues.
                    // * If we reject, then the serialization error will only be visible in the logs (and potentially earlier check_tx results).
                    tracing::warn!(
                        error = e.to_string(),
                        "failed to decode message in proposal as ChainMessage"
                    );
                    if self.reject_malformed_proposal {
                        return Ok(false);
                    }
                }
                Ok(msg) => chain_msgs.push(msg),
            }
        }

        self.inner.process(state, chain_msgs).await
    }
}

#[async_trait]
impl<I> ExecInterpreter for BytesMessageInterpreter<I>
where
    I: ExecInterpreter<Message = ChainMessage, DeliverOutput = ChainMessageApplyRet>,
{
    type State = I::State;
    type Message = Vec<u8>;
    type BeginOutput = I::BeginOutput;
    type DeliverOutput = BytesMessageApplyRes;
    type EndOutput = I::EndOutput;

    async fn deliver(
        &self,
        state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<(Self::State, Self::DeliverOutput)> {
        match fvm_ipld_encoding::from_slice::<ChainMessage>(&msg) {
            Err(e) =>
            // TODO: Punish the validator for including rubbish.
            // There is always the possibility that our codebase is incompatible,
            // but then we'll have a consensus failure later when we don't agree on the ledger.
            {
                if self.reject_malformed_proposal {
                    // We could consider panicking here, otherwise if the majority executes this transaction (they voted for it)
                    // then we will just get a consensu failure after the block.
                    tracing::warn!(
                        error = e.to_string(),
                        "failed to decode delivered message as ChainMessage; we did not vote for it, maybe our node is buggy?"
                    );
                }
                Ok((state, Err(e)))
            }
            Ok(msg) => {
                let (state, ret) = self.inner.deliver(state, msg).await?;
                Ok((state, Ok(ret)))
            }
        }
    }

    async fn begin(&self, state: Self::State) -> anyhow::Result<(Self::State, Self::BeginOutput)> {
        self.inner.begin(state).await
    }

    async fn end(&self, state: Self::State) -> anyhow::Result<(Self::State, Self::EndOutput)> {
        self.inner.end(state).await
    }
}

#[async_trait]
impl<I> CheckInterpreter for BytesMessageInterpreter<I>
where
    I: CheckInterpreter<Message = ChainMessage, Output = ChainMessageCheckRes>,
{
    type State = I::State;
    type Message = Vec<u8>;
    type Output = BytesMessageCheckRes;

    async fn check(
        &self,
        state: Self::State,
        msg: Self::Message,
        is_recheck: bool,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        match fvm_ipld_encoding::from_slice::<ChainMessage>(&msg) {
            Err(e) =>
            // The user sent us an invalid message, all we can do is discard it and block the source.
            {
                Ok((state, Err(e)))
            }
            Ok(msg) => {
                let (state, ret) = self.inner.check(state, msg, is_recheck).await?;
                Ok((state, Ok(ret)))
            }
        }
    }
}

#[async_trait]
impl<I> QueryInterpreter for BytesMessageInterpreter<I>
where
    I: QueryInterpreter<Query = FvmQuery, Output = FvmQueryRet>,
{
    type State = I::State;
    type Query = BytesMessageQuery;
    type Output = BytesMessageQueryRes;

    async fn query(
        &self,
        state: Self::State,
        qry: Self::Query,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        let (path, bz) = qry;
        let qry = if path.as_str() == "/store" {
            // According to the docstrings, the application MUST interpret `/store` as a query on the underlying KV store.
            match fvm_ipld_encoding::from_slice::<Cid>(&bz) {
                Err(e) => return Ok((state, Err(e))),
                Ok(cid) => FvmQuery::Ipld(cid),
            }
        } else {
            // Otherwise ignore the path for now. The docs also say that the query bytes can be used in lieu of the path,
            // so it's okay to have two ways to send IPLD queries: either by using the `/store` path and sending a CID,
            // or by sending the appropriate `FvmQuery`.
            match fvm_ipld_encoding::from_slice::<FvmQuery>(&bz) {
                Err(e) => return Ok((state, Err(e))),
                Ok(qry) => qry,
            }
        };

        let (state, ret) = self.inner.query(state, qry).await?;

        Ok((state, Ok(ret)))
    }
}

#[async_trait]
impl<I> GenesisInterpreter for BytesMessageInterpreter<I>
where
    I: GenesisInterpreter<Genesis = Genesis>,
{
    type State = I::State;
    type Genesis = Vec<u8>;
    type Output = I::Output;

    async fn init(
        &self,
        state: Self::State,
        genesis: Self::Genesis,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        // TODO (IPC-44): Handle the serialized application state as well as `Genesis`.
        let genesis: Genesis = parse_genesis(&genesis)?;
        self.inner.init(state, genesis).await
    }
}

#[async_trait]
impl<I> ExtendVoteInterpreter for BytesMessageInterpreter<I>
where
    I: ExtendVoteInterpreter,
{
    type State = I::State;
    type Message = I::Message;
    type Output = I::Output;

    fn extend_vote(&self, msg: Self::Message) -> anyhow::Result<Self::Output> {
        self.inner.extend_vote(msg)
    }

    async fn verify_vote_extension(
        &self,
        state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        self.inner.verify_vote_extension(state, msg).await
    }
}

/// Parse the initial genesis either as JSON or CBOR.
fn parse_genesis(bytes: &[u8]) -> anyhow::Result<Genesis> {
    try_parse_genesis_json(bytes).or_else(|e1| {
        try_parse_genesis_cbor(bytes)
            .map_err(|e2| anyhow!("failed to deserialize genesis as JSON or CBOR: {e1}; {e2}"))
    })
}

fn try_parse_genesis_json(bytes: &[u8]) -> anyhow::Result<Genesis> {
    let json = String::from_utf8(bytes.to_vec())?;
    let genesis = serde_json::from_str(&json)?;
    Ok(genesis)
}

fn try_parse_genesis_cbor(bytes: &[u8]) -> anyhow::Result<Genesis> {
    let genesis = fvm_ipld_encoding::from_slice(bytes)?;
    Ok(genesis)
}
