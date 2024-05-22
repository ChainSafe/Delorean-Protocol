// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use async_trait::async_trait;

pub mod bytes;
pub mod chain;
pub mod fvm;
pub mod signed;

#[cfg(feature = "arb")]
mod arb;

/// Initialize the chain state.
///
/// This could be from the original genesis file, or perhaps a checkpointed snapshot.
#[async_trait]
pub trait GenesisInterpreter: Sync + Send {
    type State: Send;
    type Genesis: Send;
    type Output;

    /// Initialize the chain.
    async fn init(
        &self,
        state: Self::State,
        genesis: Self::Genesis,
    ) -> anyhow::Result<(Self::State, Self::Output)>;
}

/// Prepare and process transaction proposals.
#[async_trait]
pub trait ProposalInterpreter: Sync + Send {
    /// State reflects the circumstances under which transactions were proposed, e.g. block height,
    /// but also any application specific mempool, for example one we can use to resolve CIDs
    /// in the background.
    ///
    /// State is considered read-only, since the proposal might not go through. It should only be
    /// modified by the delivery of transactions in a finalized bloc; for example that is where
    /// we would clear out data from our mempool.
    type State: Send;
    type Message: Send;

    /// Called when the current validator is about to propose a block.
    ///
    /// This is our chance to inject other transactions from our own mempool which we are now able to execute.
    async fn prepare(
        &self,
        state: Self::State,
        msgs: Vec<Self::Message>,
    ) -> anyhow::Result<Vec<Self::Message>>;

    /// Called when the current validator needs to decide whether to vote for a block.
    ///
    /// This is our chance check whether CIDs proposed for execution are available.
    ///
    /// Return `true` if we can accept this block, `false` to reject it.
    async fn process(&self, state: Self::State, msgs: Vec<Self::Message>) -> anyhow::Result<bool>;
}

/// The `ExecInterpreter` applies messages on some state, which is
/// tied to the lifecycle of a block in the ABCI.
///
/// By making it generic, the intention is that interpreters can
/// be stacked, changing the type of message along the way. For
/// example on the outermost layer the input message can be a mix
/// of self-contained messages and CIDs proposed for resolution
/// or execution, while in the innermost layer it's all self-contained.
/// Some interpreters would act like middlewares to resolve CIDs into
/// a concrete message.
///
/// The execution is asynchronous, so that the middleware is allowed
/// to potentially interact with the outside world. If this was restricted
/// to things like scheduling a CID resolution, we could use effects
/// returned from message processing. However, when a node is catching
/// up with the chain others have already committed, they have to do the
/// message resolution synchronously, so it has to be done during
/// message processing. Alternatively we'd have to split the processing
/// into async steps to pre-process the message, then synchronous steps
/// to update the state. But this approach is more flexible, because
/// the middlewares can decide on a message-by-message basis whether
/// to forward the message to the inner layer. Unfortunately block-level
/// pre-processing is not possible, because we are fed the messages
/// one by one through the ABCI.
///
/// There is no separate type for `Error`, only `Output`. The reason
/// is that we'll be calling high level executors internally that
/// already have their internal error handling, returning all domain
/// errors such as `OutOfGas` in their output, and only using the
/// error case for things that are independent of the message itself,
/// signalling unexpected problems there's no recovering from and
/// that should stop the block processing altogether.
#[async_trait]
pub trait ExecInterpreter: Sync + Send {
    type State: Send;
    type Message: Send;
    type BeginOutput;
    type DeliverOutput;
    type EndOutput;

    /// Called once at the beginning of a block.
    ///
    /// This is our chance to to run `cron` jobs for example.
    async fn begin(&self, state: Self::State) -> anyhow::Result<(Self::State, Self::BeginOutput)>;

    /// Apply a message onto the state.
    ///
    /// The state is taken by value, so there's no issue with sharing
    /// mutable references in futures. The modified value should be
    /// returned along with the return value.
    ///
    /// Only return an error case if something truly unexpected happens
    /// that should stop message processing altogether; otherwise use
    /// the output for signalling all execution results.
    async fn deliver(
        &self,
        state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<(Self::State, Self::DeliverOutput)>;

    /// Called once at the end of a block.
    ///
    /// This is where we can apply end-of-epoch processing, for example to process staking
    /// requests once every 1000 blocks.
    async fn end(&self, state: Self::State) -> anyhow::Result<(Self::State, Self::EndOutput)>;
}

/// Check if messages can be added to the mempool by performing certain validation
/// over a projected version of the state. Does not execute transactions fully,
/// just does basic validation. The state is updated so that things like nonces
/// and balances are adjusted as if the transaction was executed. This way an
/// account can send multiple messages in a row, not just the next that follows
/// its current nonce.
#[async_trait]
pub trait CheckInterpreter: Sync + Send {
    type State: Send;
    type Message: Send;
    type Output;

    /// Called when a new user transaction is being added to the mempool.
    ///
    /// Returns the updated state, and the check output, which should be
    /// able to describe both the success and failure cases.
    ///
    /// The recheck flags indicates that we are checking the transaction
    /// again because we have seen a new block and the state changed.
    /// As an optimisation, checks that do not depend on state can be skipped.
    async fn check(
        &self,
        state: Self::State,
        msg: Self::Message,
        is_recheck: bool,
    ) -> anyhow::Result<(Self::State, Self::Output)>;
}

/// Run a query over the ledger.
#[async_trait]
pub trait QueryInterpreter: Sync + Send {
    type State: Send;
    type Query: Send;
    type Output;

    /// Run a single query against the state.
    ///
    /// It takes and returns the state in case we wanted to do some caching of
    /// things which otherwise aren't safe to send over async boundaries.
    async fn query(
        &self,
        state: Self::State,
        qry: Self::Query,
    ) -> anyhow::Result<(Self::State, Self::Output)>;
}
