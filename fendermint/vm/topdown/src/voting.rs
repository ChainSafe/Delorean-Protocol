// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use async_stm::{abort, atomically_or_err, retry, Stm, StmResult, TVar};
use serde::{de::DeserializeOwned, Serialize};
use std::hash::Hash;
use std::{fmt::Debug, time::Duration};

use crate::{BlockHash, BlockHeight};

// Usign this type because it's `Hash`, unlike the normal `libsecp256k1::PublicKey`.
pub use ipc_ipld_resolver::ValidatorKey;
use ipc_ipld_resolver::VoteRecord;

pub type Weight = u64;

#[derive(Debug, thiserror::Error)]
pub enum Error<K = ValidatorKey, V: AsRef<[u8]> = BlockHash> {
    #[error("the last finalized block has not been set")]
    Uninitialized,

    #[error("failed to extend chain; expected block height {0}, got {1}")]
    UnexpectedBlock(BlockHeight, BlockHeight),

    #[error("validator unknown or has no power: {0:?}")]
    UnpoweredValidator(K),

    #[error(
        "equivocation by validator {0:?} at height {1}; {} != {}",
        hex::encode(.2),
        hex::encode(.3)
    )]
    Equivocation(K, BlockHeight, V, V),
}

/// Keep track of votes being gossiped about parent chain finality
/// and tally up the weights of the validators on the child subnet,
/// so that we can ask for proposals that are not going to be voted
/// down.
#[derive(Clone)]
pub struct VoteTally<K = ValidatorKey, V = BlockHash> {
    /// Current validator weights. These are the ones who will vote on the blocks,
    /// so these are the weights which need to form a quorum.
    power_table: TVar<im::HashMap<K, Weight>>,

    /// The *finalized mainchain* of the parent as observed by this node.
    ///
    /// These are assumed to be final because IIRC that's how the syncer works,
    /// only fetching the info about blocks which are already sufficiently deep.
    ///
    /// When we want to propose, all we have to do is walk back this chain and
    /// tally the votes we collected for the block hashes until we reach a quorum.
    ///
    /// The block hash is optional to allow for null blocks on Filecoin rootnet.
    chain: TVar<im::OrdMap<BlockHeight, Option<V>>>,

    /// Index votes received by height and hash, which makes it easy to look up
    /// all the votes for a given block hash and also to verify that a validator
    /// isn't equivocating by trying to vote for two different things at the
    /// same height.
    votes: TVar<im::OrdMap<BlockHeight, im::HashMap<V, im::HashSet<K>>>>,

    /// Adding votes can be paused if we observe that looking for a quorum takes too long
    /// and is often retried due to votes being added.
    pause_votes: TVar<bool>,
}

impl<K, V> VoteTally<K, V>
where
    K: Clone + Hash + Eq + Sync + Send + 'static + Debug,
    V: AsRef<[u8]> + Clone + Hash + Eq + Sync + Send + 'static,
{
    /// Create an uninitialized instance. Before blocks can be added to it
    /// we will have to set the last finalized block.
    ///
    /// The reason this exists is so that we can delay initialization until
    /// after the genesis block has been executed.
    pub fn empty() -> Self {
        Self {
            power_table: TVar::default(),
            chain: TVar::default(),
            votes: TVar::default(),
            pause_votes: TVar::new(false),
        }
    }

    /// Initialize the vote tally from the current power table
    /// and the last finalized block from the ledger.
    pub fn new(power_table: Vec<(K, Weight)>, last_finalized_block: (BlockHeight, V)) -> Self {
        let (height, hash) = last_finalized_block;
        Self {
            power_table: TVar::new(im::HashMap::from_iter(power_table)),
            chain: TVar::new(im::OrdMap::from_iter([(height, Some(hash))])),
            votes: TVar::default(),
            pause_votes: TVar::new(false),
        }
    }

    /// Check that a validator key is currently part of the power table.
    pub fn has_power(&self, validator_key: &K) -> Stm<bool> {
        let pt = self.power_table.read()?;
        // For consistency consider validators without power unknown.
        match pt.get(validator_key) {
            None => Ok(false),
            Some(weight) => Ok(*weight > 0),
        }
    }

    /// Calculate the minimum weight needed for a proposal to pass with the current membership.
    ///
    /// This is inclusive, that is, if the sum of weight is greater or equal to this, it should pass.
    /// The equivalent formula can be found in CometBFT [here](https://github.com/cometbft/cometbft/blob/a8991d63e5aad8be82b90329b55413e3a4933dc0/types/vote_set.go#L307).
    pub fn quorum_threshold(&self) -> Stm<Weight> {
        let total_weight: Weight = self.power_table.read().map(|pt| pt.values().sum())?;

        Ok(total_weight * 2 / 3 + 1)
    }

    /// Return the height of the first entry in the chain.
    ///
    /// This is the block that was finalized *in the ledger*.
    pub fn last_finalized_height(&self) -> Stm<BlockHeight> {
        self.chain
            .read()
            .map(|c| c.get_min().map(|(h, _)| *h).unwrap_or_default())
    }

    /// Return the height of the last entry in the chain.
    ///
    /// This is the block that we can cast our vote on as final.
    pub fn latest_height(&self) -> Stm<BlockHeight> {
        self.chain
            .read()
            .map(|c| c.get_max().map(|(h, _)| *h).unwrap_or_default())
    }

    /// Get the hash of a block at the given height, if known.
    pub fn block_hash(&self, height: BlockHeight) -> Stm<Option<V>> {
        self.chain.read().map(|c| c.get(&height).cloned().flatten())
    }

    /// Add the next final block observed on the parent blockchain.
    ///
    /// Returns an error unless it's exactly the next expected height,
    /// so the caller has to call this in every epoch. If the parent
    /// chain produced no blocks in that epoch then pass `None` to
    /// represent that null-round in the tally.
    pub fn add_block(
        &self,
        block_height: BlockHeight,
        block_hash: Option<V>,
    ) -> StmResult<(), Error<K>> {
        let mut chain = self.chain.read_clone()?;

        // Check that we are extending the chain. We could also ignore existing heights.
        match chain.get_max() {
            None => {
                return abort(Error::Uninitialized);
            }
            Some((parent_height, _)) => {
                if block_height != parent_height + 1 {
                    return abort(Error::UnexpectedBlock(parent_height + 1, block_height));
                }
            }
        }

        chain.insert(block_height, block_hash);

        self.chain.write(chain)?;

        Ok(())
    }

    /// Add a vote we received.
    ///
    /// Returns `true` if this vote was added, `false` if it was ignored as a
    /// duplicate or a height we already finalized, and an error if it's an
    /// equivocation or from a validator we don't know.
    pub fn add_vote(
        &self,
        validator_key: K,
        block_height: BlockHeight,
        block_hash: V,
    ) -> StmResult<bool, Error<K, V>> {
        if *self.pause_votes.read()? {
            retry()?;
        }

        let min_height = self.last_finalized_height()?;

        if block_height < min_height {
            return Ok(false);
        }

        if !self.has_power(&validator_key)? {
            return abort(Error::UnpoweredValidator(validator_key));
        }

        let mut votes = self.votes.read_clone()?;
        let votes_at_height = votes.entry(block_height).or_default();

        for (bh, vs) in votes_at_height.iter() {
            if *bh != block_hash && vs.contains(&validator_key) {
                return abort(Error::Equivocation(
                    validator_key,
                    block_height,
                    block_hash,
                    bh.clone(),
                ));
            }
        }

        let votes_for_block = votes_at_height.entry(block_hash).or_default();

        if votes_for_block.insert(validator_key).is_some() {
            return Ok(false);
        }

        self.votes.write(votes)?;

        Ok(true)
    }

    /// Pause adding more votes until we are finished calling `find_quorum` which
    /// automatically re-enables them.
    pub fn pause_votes_until_find_quorum(&self) -> Stm<()> {
        self.pause_votes.write(true)
    }

    /// Find a block on the (from our perspective) finalized chain that gathered enough votes from validators.
    pub fn find_quorum(&self) -> Stm<Option<(BlockHeight, V)>> {
        self.pause_votes.write(false)?;

        let quorum_threshold = self.quorum_threshold()?;
        let chain = self.chain.read()?;

        let Some((finalized_height, _)) = chain.get_min() else {
            tracing::debug!("finalized height not found");
            return Ok(None);
        };

        let votes = self.votes.read()?;
        let power_table = self.power_table.read()?;

        let mut weight = 0;
        let mut voters = im::HashSet::new();

        for (block_height, block_hash) in chain.iter().rev() {
            if block_height == finalized_height {
                tracing::debug!(
                    block_height,
                    finalized_height,
                    "finalized height and block height equal, no new proposals"
                );
                break; // This block is already finalized in the ledger, no need to propose it again.
            }
            let Some(block_hash) = block_hash else {
                tracing::debug!(block_height, "null block found in vote proposal");
                continue; // Skip null blocks
            };
            let Some(votes_at_height) = votes.get(block_height) else {
                tracing::debug!(block_height, "no votes");
                continue;
            };
            let Some(votes_for_block) = votes_at_height.get(block_hash) else {
                tracing::debug!(block_height, "no votes for block");
                continue; // We could detect equovicating voters here.
            };

            for vk in votes_for_block {
                if voters.insert(vk.clone()).is_none() {
                    // New voter, get their current weight; it might be 0 if they have been removed.
                    weight += power_table.get(vk).cloned().unwrap_or_default();
                    tracing::debug!(weight, key = ?vk, "new voter");
                }
            }

            tracing::debug!(weight, quorum_threshold, "showdown");

            if weight >= quorum_threshold {
                return Ok(Some((*block_height, block_hash.clone())));
            }
        }

        Ok(None)
    }

    /// Call when a new finalized block is added to the ledger, to clear out all preceding blocks.
    ///
    /// After this operation the minimum item in the chain will the new finalized block.
    pub fn set_finalized(&self, block_height: BlockHeight, block_hash: V) -> Stm<()> {
        self.chain.update(|chain| {
            let (_, mut chain) = chain.split(&block_height);
            chain.insert(block_height, Some(block_hash));
            chain
        })?;

        self.votes.update(|votes| votes.split(&block_height).1)?;

        Ok(())
    }

    /// Overwrite the power table after it has changed to a new snapshot.
    ///
    /// This method expects absolute values, it completely replaces the existing powers.
    pub fn set_power_table(&self, power_table: Vec<(K, Weight)>) -> Stm<()> {
        let power_table = im::HashMap::from_iter(power_table);
        // We don't actually have to remove the votes of anyone who is no longer a validator,
        // we just have to make sure to handle the case when they are not in the power table.
        self.power_table.write(power_table)
    }

    /// Update the power table after it has changed with changes.
    ///
    /// This method expects only the updated values, leaving everyone who isn't in it untouched
    pub fn update_power_table(&self, power_updates: Vec<(K, Weight)>) -> Stm<()> {
        if power_updates.is_empty() {
            return Ok(());
        }
        // We don't actually have to remove the votes of anyone who is no longer a validator,
        // we just have to make sure to handle the case when they are not in the power table.
        self.power_table.update_mut(|pt| {
            for (vk, w) in power_updates {
                if w == 0 {
                    pt.remove(&vk);
                } else {
                    *pt.entry(vk).or_default() = w;
                }
            }
        })
    }
}

/// Poll the vote tally for new finalized blocks and publish a vote about them if the validator is part of the power table.
pub async fn publish_vote_loop<V, F>(
    vote_tally: VoteTally,
    // Throttle votes to maximum 1/interval
    vote_interval: Duration,
    // Publish a vote after a timeout even if it's the same as before.
    vote_timeout: Duration,
    key: libp2p::identity::Keypair,
    subnet_id: ipc_api::subnet_id::SubnetID,
    client: ipc_ipld_resolver::Client<V>,
    to_vote: F,
) where
    F: Fn(BlockHeight, BlockHash) -> V,
    V: Serialize + DeserializeOwned,
{
    let validator_key = ValidatorKey::from(key.public());

    let mut vote_interval = tokio::time::interval(vote_interval);
    vote_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut prev = None;

    loop {
        let prev_height = prev
            .as_ref()
            .map(|(height, _, _)| *height)
            .unwrap_or_default();

        let result = tokio::time::timeout(
            vote_timeout,
            atomically_or_err(|| {
                let next_height = vote_tally.latest_height()?;

                if next_height == prev_height {
                    retry()?;
                }

                let next_hash = match vote_tally.block_hash(next_height)? {
                    Some(next_hash) => next_hash,
                    None => retry()?,
                };

                let has_power = vote_tally.has_power(&validator_key)?;

                if has_power {
                    // Add our own vote to the tally directly rather than expecting a message from the gossip channel.
                    // TODO (ENG-622): I'm not sure gossip messages published by this node would be delivered to it, so this might be the only way.
                    // NOTE: We should not see any other error from this as we just checked that the validator had power,
                    //       but for piece of mind let's return and log any potential errors, rather than ignore them.
                    vote_tally.add_vote(validator_key.clone(), next_height, next_hash.clone())?;
                }

                Ok((next_height, next_hash, has_power))
            }),
        )
        .await;

        let (next_height, next_hash, has_power) = match result {
            Ok(Ok(vs)) => vs,
            Err(_) => {
                if let Some(ref vs) = prev {
                    tracing::debug!("vote timeout; re-publishing previous vote");
                    vs.clone()
                } else {
                    tracing::debug!("vote timeout, but no previous vote to re-publish");
                    continue;
                }
            }
            Ok(Err(e)) => {
                tracing::error!(
                    error = e.to_string(),
                    "failed to get next height to vote on"
                );
                continue;
            }
        };

        if has_power && prev_height > 0 {
            tracing::debug!(block_height = next_height, "publishing finality vote");

            let vote = to_vote(next_height, next_hash.clone());

            match VoteRecord::signed(&key, subnet_id.clone(), vote) {
                Ok(vote) => {
                    if let Err(e) = client.publish_vote(vote) {
                        tracing::error!(error = e.to_string(), "failed to publish vote");
                    }
                }
                Err(e) => {
                    tracing::error!(error = e.to_string(), "failed to sign vote");
                }
            }

            // Throttle vote gossiping at periods of fast syncing. For example if we create a subnet contract on Friday
            // and bring up a local testnet on Monday, all nodes would be ~7000 blocks behind a Lotus parent. CometBFT
            // would be in-sync, and they could rapidly try to gossip votes on previous heights. GossipSub might not like
            // that, and we can just cast our votes every now and then to finalize multiple blocks.
            vote_interval.tick().await;
        }

        prev = Some((next_height, next_hash, has_power));
    }
}
