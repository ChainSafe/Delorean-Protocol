// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

//! State Machine Test for the finality voting tally component.
//!
//! The test simulates random events that the tally can receive, such as votes received
//! over gossip, power table updates, block being executed, and tests that the tally
//! correctly identifies the blocks which are agreeable to the majority of validator.
//!
//! It can be executed the following way:
//!
//! ```text
//! cargo test --release -p fendermint_vm_topdown --test smt_voting
//! ```

use std::{
    cmp::{max, min},
    collections::BTreeMap,
    fmt::Debug,
};

use arbitrary::Unstructured;
use async_stm::{atomically, atomically_or_err, Stm, StmResult};
use fendermint_testing::{smt, state_machine_test};
use fendermint_vm_topdown::{
    voting::{self, VoteTally, Weight},
    BlockHash, BlockHeight,
};
use im::HashSet;
//use rand::{rngs::StdRng, SeedableRng};

/// Size of window of voting relative to the last cast vote.
const MAX_VOTE_DELTA: BlockHeight = 5;
/// Maximum number of blocks to finalize at a time.
const MAX_FINALIZED_DELTA: BlockHeight = 5;

state_machine_test!(voting, 10000 ms, 65512 bytes, 200 steps, VotingMachine::new());
//state_machine_test!(voting, 0xf7ac11a50000ffe8, 200 steps, VotingMachine::new());

/// Test key to make debugging more readable.
#[derive(Debug, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct VotingKey(u64);

pub type VotingError = voting::Error<VotingKey>;

pub enum VotingCommand {
    /// The tally observes the next block fo the chain.
    ExtendChain(BlockHeight, Option<BlockHash>),
    /// One of the validators voted on a block.
    AddVote(VotingKey, BlockHeight, BlockHash),
    /// Update the power table.
    UpdatePower(Vec<(VotingKey, Weight)>),
    /// A certain height was finalized in the ledger.
    BlockFinalized(BlockHeight, BlockHash),
    /// Ask the tally for the highest agreeable block.
    FindQuorum,
}

// Debug format without block hashes which make it unreadable.
impl Debug for VotingCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExtendChain(arg0, arg1) => f
                .debug_tuple("ExtendChain")
                .field(arg0)
                .field(&arg1.is_some())
                .finish(),
            Self::AddVote(arg0, arg1, _arg2) => {
                f.debug_tuple("AddVote").field(arg0).field(arg1).finish()
            }
            Self::UpdatePower(arg0) => f.debug_tuple("UpdatePower").field(arg0).finish(),
            Self::BlockFinalized(arg0, _arg1) => {
                f.debug_tuple("BlockFinalized").field(arg0).finish()
            }
            Self::FindQuorum => write!(f, "FindQuorum"),
        }
    }
}

/// Model state of voting
#[derive(Clone)]
pub struct VotingState {
    /// We have a single parent chain that everybody observes, just at different heights.
    /// There is no forking in this test because we assume that the syncing component
    /// only downloads blocks which are final, and that reorgs don't happen.
    ///
    /// Null blocks are represented by `None`.
    ///
    /// The tally is currently unable to handle reorgs and rejects equivocations anyway.
    ///
    /// TODO (ENG-623): Decide what we want to achieve with Equivocation detection.
    chain: Vec<Option<BlockHash>>,
    /// All the validator keys to help pic random ones.
    validator_keys: Vec<VotingKey>,
    /// All the validators with varying weights (can be zero).
    validator_states: BTreeMap<VotingKey, ValidatorState>,

    last_finalized_block: BlockHeight,
    last_chain_block: BlockHeight,
}

impl VotingState {
    pub fn can_extend(&self) -> bool {
        self.last_chain_block < self.max_chain_height()
    }

    pub fn can_finalize(&self) -> bool {
        // We can finalize a block even if we haven't observed the votes,
        // if the majority of validators vote for an actual block that
        // proposed it for execution.
        self.last_finalized_block < self.max_chain_height()
    }

    pub fn next_chain_block(&self) -> Option<(BlockHeight, Option<BlockHash>)> {
        if self.can_extend() {
            let h = self.last_chain_block + 1;
            Some((h, self.block_hash(h)))
        } else {
            None
        }
    }

    pub fn max_chain_height(&self) -> BlockHeight {
        self.chain.len() as BlockHeight - 1
    }

    pub fn block_hash(&self, h: BlockHeight) -> Option<BlockHash> {
        self.chain[h as usize].clone()
    }

    pub fn has_quorum(&self, h: BlockHeight) -> bool {
        if self.block_hash(h).is_none() {
            return false;
        }

        let mut total_weight: Weight = 0;
        let mut vote_weight: Weight = 0;

        for vs in self.validator_states.values() {
            total_weight += vs.weight;
            if vs.highest_vote >= h {
                vote_weight += vs.weight;
            }
        }

        let threshold = total_weight * 2 / 3;

        vote_weight > threshold
    }
}

#[derive(Clone, Debug)]
pub struct ValidatorState {
    /// Current voting power (can be zero).
    weight: Weight,
    /// The heights this validator explicitly voted on.
    votes: HashSet<BlockHeight>,
    /// The highest vote *currently on the chain* the validator has voted for already.
    /// Initially zero, meaning everyone voted on the initial finalized block.
    highest_vote: BlockHeight,
}

pub struct VotingMachine {
    /// Runtime for executing async commands.
    runtime: tokio::runtime::Runtime,
}

impl VotingMachine {
    pub fn new() -> Self {
        Self {
            runtime: tokio::runtime::Runtime::new().expect("create tokio runtime"),
        }
    }

    fn atomically_or_err<F, T>(&self, f: F) -> Result<T, VotingError>
    where
        F: Fn() -> StmResult<T, VotingError>,
    {
        self.runtime.block_on(atomically_or_err(f))
    }

    fn atomically<F, T>(&self, f: F) -> T
    where
        F: Fn() -> Stm<T>,
    {
        self.runtime.block_on(atomically(f))
    }

    // For convenience in the command handler.
    fn atomically_ok<F, T>(&self, f: F) -> Result<T, VotingError>
    where
        F: Fn() -> Stm<T>,
    {
        Ok(self.atomically(f))
    }
}

impl Default for VotingMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl smt::StateMachine for VotingMachine {
    /// The System Under Test is the Vote Tally.
    type System = VoteTally<VotingKey>;
    /// The model state is defined here in the test.
    type State = VotingState;
    /// Random commands we can apply in a step.
    type Command = VotingCommand;
    /// Result of command application on the system.
    ///
    /// The only return value we are interested in is the finality.
    type Result = Result<Option<(BlockHeight, BlockHash)>, voting::Error<VotingKey>>;

    /// New random state.
    fn gen_state(&self, u: &mut Unstructured) -> arbitrary::Result<Self::State> {
        let chain_length = u.int_in_range(40..=60)?;
        let mut chain = Vec::new();
        for i in 0..chain_length {
            if i == 0 || u.ratio(9, 10)? {
                let block_hash = u.bytes(32)?;
                chain.push(Some(Vec::from(block_hash)));
            } else {
                chain.push(None);
            }
        }

        let validator_count = u.int_in_range(1..=5)?;
        //let mut rng = StdRng::seed_from_u64(u.arbitrary()?);
        let mut validator_states = BTreeMap::new();

        for i in 0..validator_count {
            let min_weight = if i == 0 { 1u64 } else { 0u64 };
            let weight = u.int_in_range(min_weight..=100)?;

            // A VotingKey is has a lot of wrapping...
            // let secret_key = fendermint_crypto::SecretKey::random(&mut rng);
            // let public_key = secret_key.public_key();
            // let public_key = libp2p::identity::secp256k1::PublicKey::try_from_bytes(
            //     &public_key.serialize_compressed(),
            // )
            // .expect("secp256k1 public key");
            // let public_key = libp2p::identity::PublicKey::from(public_key);
            // let validator_key = VotingKey::from(public_key);

            let validator_key = VotingKey(i);

            validator_states.insert(
                validator_key,
                ValidatorState {
                    weight,
                    votes: HashSet::default(),
                    highest_vote: 0,
                },
            );
        }

        eprintln!("NEW STATE: {validator_states:?}");

        Ok(VotingState {
            chain,
            validator_keys: validator_states.keys().cloned().collect(),
            validator_states,
            last_chain_block: 0,
            last_finalized_block: 0,
        })
    }

    /// New System Under Test.
    fn new_system(&self, state: &Self::State) -> Self::System {
        let power_table = state
            .validator_states
            .iter()
            .filter(|(_, vs)| vs.weight > 0)
            .map(|(vk, vs)| (vk.clone(), vs.weight))
            .collect();

        let last_finalized_block = (0, state.block_hash(0).expect("first block is not null"));

        VoteTally::<VotingKey>::new(power_table, last_finalized_block)
    }

    /// New random command.
    fn gen_command(
        &self,
        u: &mut Unstructured,
        state: &Self::State,
    ) -> arbitrary::Result<Self::Command> {
        let cmd = match u.int_in_range(0..=100)? {
            // Add a block to the observed chain
            i if i < 25 && state.can_extend() => {
                let (height, hash) = state.next_chain_block().unwrap();
                VotingCommand::ExtendChain(height, hash)
            }
            // Add a new (or repeated) vote by a validator, extending its chain
            i if i < 70 => {
                let vk = u.choose(&state.validator_keys)?;
                let high_vote = state.validator_states[vk].highest_vote;
                let max_vote: BlockHeight =
                    min(state.max_chain_height(), high_vote + MAX_VOTE_DELTA);
                let min_vote: BlockHeight = high_vote.saturating_sub(MAX_VOTE_DELTA);

                let mut vote_height = u.int_in_range(min_vote..=max_vote)?;
                while state.block_hash(vote_height).is_none() {
                    vote_height -= 1;
                }
                let vote_hash = state
                    .block_hash(vote_height)
                    .expect("the first block not null");

                VotingCommand::AddVote(vk.clone(), vote_height, vote_hash)
            }
            // Update the power table
            i if i < 80 => {
                // Move power from one validator to another (so we never have everyone be zero).
                let vk1 = u.choose(&state.validator_keys)?;
                let vk2 = u.choose(&state.validator_keys)?;
                let w1 = state.validator_states[vk1].weight;
                let w2 = state.validator_states[vk2].weight;
                let delta = u.int_in_range(0..=w1)?;

                let updates = vec![(vk1.clone(), w1 - delta), (vk2.clone(), w2 + delta)];

                VotingCommand::UpdatePower(updates)
            }
            // Finalize a block
            i if i < 90 && state.can_finalize() => {
                let min_fin = state.last_finalized_block + 1;
                let max_fin = min(
                    state.max_chain_height(),
                    state.last_finalized_block + MAX_FINALIZED_DELTA,
                );

                let mut fin_height = u.int_in_range(min_fin..=max_fin)?;
                while state.block_hash(fin_height).is_none() {
                    fin_height -= 1;
                }
                let fin_hash = state
                    .block_hash(fin_height)
                    .expect("the first block not null");

                // Might be a duplicate, which doesn't happen in the real ledger, but it's okay.
                VotingCommand::BlockFinalized(fin_height, fin_hash)
            }
            _ => VotingCommand::FindQuorum,
        };
        Ok(cmd)
    }

    /// Apply the command on the System Under Test.
    fn run_command(&self, system: &mut Self::System, cmd: &Self::Command) -> Self::Result {
        eprintln!("RUN CMD {cmd:?}");
        match cmd {
            VotingCommand::ExtendChain(block_height, block_hash) => self.atomically_or_err(|| {
                system
                    .add_block(*block_height, block_hash.clone())
                    .map(|_| None)
            }),
            VotingCommand::AddVote(vk, block_height, block_hash) => self.atomically_or_err(|| {
                system
                    .add_vote(vk.clone(), *block_height, block_hash.clone())
                    .map(|_| None)
            }),

            VotingCommand::UpdatePower(power_table) => {
                self.atomically_ok(|| system.update_power_table(power_table.clone()).map(|_| None))
            }

            VotingCommand::BlockFinalized(block_height, block_hash) => self.atomically_ok(|| {
                system
                    .set_finalized(*block_height, block_hash.clone())
                    .map(|_| None)
            }),

            VotingCommand::FindQuorum => self.atomically_ok(|| system.find_quorum()),
        }
    }

    /// Check that the result returned by the tally is correct.
    fn check_result(&self, cmd: &Self::Command, pre_state: &Self::State, result: Self::Result) {
        match cmd {
            VotingCommand::ExtendChain(_, _) => {
                result.expect("chain extension should succeed; not simulating unexpected heights");
            }
            VotingCommand::AddVote(vk, h, _) => {
                if *h < pre_state.last_finalized_block {
                    result.expect("old votes are ignored");
                } else if pre_state.validator_states[vk].weight == 0 {
                    result.expect_err("not accepting votes from validators with 0 power");
                } else {
                    result.expect("vote should succeed; not simulating equivocations");
                }
            }
            VotingCommand::FindQuorum => {
                let result = result.expect("finding quorum should succeed");

                let height = match result {
                    None => pre_state.last_finalized_block,
                    Some((height, hash)) => {
                        assert!(
                            pre_state.has_quorum(height),
                            "find: height {height} should have quorum"
                        );
                        assert!(
                            height > pre_state.last_finalized_block,
                            "find: should be above last finalized"
                        );
                        assert!(
                            height <= pre_state.last_chain_block,
                            "find: should not be beyond last chain"
                        );
                        assert_eq!(
                            pre_state.block_hash(height),
                            Some(hash),
                            "find: should be correct hash"
                        );
                        height
                    }
                };

                // Check that the first non-null block after the finalized one has no quorum.
                let mut next = height + 1;
                if next > pre_state.max_chain_height() || next > pre_state.last_chain_block {
                    return;
                }
                while next < pre_state.last_chain_block && pre_state.block_hash(next).is_none() {
                    next += 1;
                }
                assert!(
                    !pre_state.has_quorum(next),
                    "next block at {next} should not have quorum"
                )
            }
            other => {
                assert!(result.is_ok(), "{other:?} should succeed: {result:?}");
            }
        }
    }

    /// Update the model state.
    fn next_state(&self, cmd: &Self::Command, mut state: Self::State) -> Self::State {
        match cmd {
            VotingCommand::ExtendChain(h, _) => {
                state.last_chain_block = *h;
                for vs in state.validator_states.values_mut() {
                    if vs.votes.contains(h) {
                        vs.highest_vote = *h;
                    }
                }
            }
            VotingCommand::AddVote(vk, h, _) => {
                let vs = state
                    .validator_states
                    .get_mut(vk)
                    .expect("validator exists");

                if vs.weight > 0 {
                    vs.votes.insert(*h);

                    if *h <= state.last_chain_block {
                        vs.highest_vote = max(vs.highest_vote, *h);
                    }
                }
            }
            VotingCommand::UpdatePower(pt) => {
                for (vk, w) in pt {
                    state
                        .validator_states
                        .get_mut(vk)
                        .expect("validators exist")
                        .weight = *w;
                }
            }
            VotingCommand::BlockFinalized(h, _) => {
                state.last_finalized_block = *h;
                state.last_chain_block = max(state.last_chain_block, state.last_finalized_block);
            }
            VotingCommand::FindQuorum => {}
        }
        state
    }

    /// Compare the tally agains the updated model state.
    fn check_system(
        &self,
        _cmd: &Self::Command,
        post_state: &Self::State,
        post_system: &Self::System,
    ) -> bool {
        let last_finalized_block = self.atomically(|| post_system.last_finalized_height());

        assert_eq!(
            last_finalized_block, post_state.last_finalized_block,
            "last finalized blocks should match"
        );

        // Stop if we finalized everything.
        last_finalized_block < post_state.max_chain_height()
    }
}
