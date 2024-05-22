// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::finality::null::FinalityWithNull;
use crate::finality::ParentViewPayload;
use crate::proxy::ParentQueryProxy;
use crate::{
    handle_null_round, BlockHash, BlockHeight, Config, Error, IPCParentFinality,
    ParentFinalityProvider, ParentViewProvider,
};
use async_stm::{Stm, StmResult};
use ipc_api::cross::IpcEnvelope;
use ipc_api::staking::StakingChangeRequest;
use std::sync::Arc;

/// The finality provider that performs io to the parent if not found in cache
#[derive(Clone)]
pub struct CachedFinalityProvider<T> {
    inner: FinalityWithNull,
    config: Config,
    /// The ipc client proxy that works as a back up if cache miss
    parent_client: Arc<T>,
}

/// Exponential backoff for futures
macro_rules! retry {
    ($wait:expr, $retires:expr, $f:expr) => {{
        let mut retries = $retires;
        let mut wait = $wait;

        loop {
            let res = $f;
            if let Err(e) = &res {
                // there is no point in retrying if the current block is null round
                if crate::is_null_round_str(&e.to_string()) {
                    tracing::warn!(
                        "cannot query ipc parent_client due to null round, skip retry"
                    );
                    break res;
                }

                tracing::warn!(
                    error = e.to_string(),
                    retries,
                    wait = ?wait,
                    "cannot query ipc parent_client"
                );

                if retries > 0 {
                    retries -= 1;

                    tokio::time::sleep(wait).await;

                    wait *= 2;
                    continue;
                }
            }

            break res;
        }
    }};
}

#[async_trait::async_trait]
impl<T: ParentQueryProxy + Send + Sync + 'static> ParentViewProvider for CachedFinalityProvider<T> {
    fn genesis_epoch(&self) -> anyhow::Result<BlockHeight> {
        self.inner.genesis_epoch()
    }

    async fn validator_changes_from(
        &self,
        from: BlockHeight,
        to: BlockHeight,
    ) -> anyhow::Result<Vec<StakingChangeRequest>> {
        let mut v = vec![];
        for h in from..=to {
            let mut r = self.validator_changes(h).await?;
            tracing::debug!(
                number_of_messages = r.len(),
                height = h,
                "fetched validator change set",
            );
            v.append(&mut r);
        }

        Ok(v)
    }

    /// Get top down message in the range `from` to `to`, both inclusive. For the check to be valid, one
    /// should not pass a height `to` that is a null block, otherwise the check is useless. In debug
    /// mode, it will throw an error.
    async fn top_down_msgs_from(
        &self,
        from: BlockHeight,
        to: BlockHeight,
    ) -> anyhow::Result<Vec<IpcEnvelope>> {
        let mut v = vec![];
        for h in from..=to {
            let mut r = self.top_down_msgs(h).await?;
            tracing::debug!(
                number_of_top_down_messages = r.len(),
                height = h,
                "obtained topdown messages",
            );
            v.append(&mut r);
        }
        Ok(v)
    }
}

impl<T: ParentQueryProxy + Send + Sync + 'static> ParentFinalityProvider
    for CachedFinalityProvider<T>
{
    fn next_proposal(&self) -> Stm<Option<IPCParentFinality>> {
        self.inner.next_proposal()
    }

    fn check_proposal(&self, proposal: &IPCParentFinality) -> Stm<bool> {
        self.inner.check_proposal(proposal)
    }

    fn set_new_finality(
        &self,
        finality: IPCParentFinality,
        previous_finality: Option<IPCParentFinality>,
    ) -> Stm<()> {
        self.inner.set_new_finality(finality, previous_finality)
    }
}

impl<T: ParentQueryProxy + Send + Sync + 'static> CachedFinalityProvider<T> {
    /// Creates an uninitialized provider
    /// We need this because `fendermint` has yet to be initialized and might
    /// not be able to provide an existing finality from the storage. This provider requires an
    /// existing committed finality. Providing the finality will enable other functionalities.
    pub async fn uninitialized(config: Config, parent_client: Arc<T>) -> anyhow::Result<Self> {
        let genesis = parent_client.get_genesis_epoch().await?;
        Ok(Self::new(config, genesis, None, parent_client))
    }

    /// Should always return the top down messages, only when ipc parent_client is down after exponential
    /// retries
    async fn validator_changes(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Vec<StakingChangeRequest>> {
        let r = self.inner.validator_changes(height).await?;

        if let Some(v) = r {
            return Ok(v);
        }

        let r = retry!(
            self.config.exponential_back_off,
            self.config.exponential_retry_limit,
            self.parent_client
                .get_validator_changes(height)
                .await
                .map(|r| r.value)
        );

        handle_null_round(r, Vec::new)
    }

    /// Should always return the top down messages, only when ipc parent_client is down after exponential
    /// retries
    async fn top_down_msgs(&self, height: BlockHeight) -> anyhow::Result<Vec<IpcEnvelope>> {
        let r = self.inner.top_down_msgs(height).await?;

        if let Some(v) = r {
            return Ok(v);
        }

        let r = retry!(
            self.config.exponential_back_off,
            self.config.exponential_retry_limit,
            self.parent_client
                .get_top_down_msgs(height)
                .await
                .map(|r| r.value)
        );

        handle_null_round(r, Vec::new)
    }
}

impl<T> CachedFinalityProvider<T> {
    pub(crate) fn new(
        config: Config,
        genesis_epoch: BlockHeight,
        committed_finality: Option<IPCParentFinality>,
        parent_client: Arc<T>,
    ) -> Self {
        let inner = FinalityWithNull::new(config.clone(), genesis_epoch, committed_finality);
        Self {
            inner,
            config,
            parent_client,
        }
    }

    pub fn block_hash(&self, height: BlockHeight) -> Stm<Option<BlockHash>> {
        self.inner.block_hash_at_height(height)
    }

    pub fn latest_height_in_cache(&self) -> Stm<Option<BlockHeight>> {
        self.inner.latest_height_in_cache()
    }

    /// Get the latest height tracked in the provider, includes both cache and last committed finality
    pub fn latest_height(&self) -> Stm<Option<BlockHeight>> {
        self.inner.latest_height()
    }

    pub fn last_committed_finality(&self) -> Stm<Option<IPCParentFinality>> {
        self.inner.last_committed_finality()
    }

    /// Clear the cache and set the committed finality to the provided value
    pub fn reset(&self, finality: IPCParentFinality) -> Stm<()> {
        self.inner.reset(finality)
    }

    pub fn new_parent_view(
        &self,
        height: BlockHeight,
        maybe_payload: Option<ParentViewPayload>,
    ) -> StmResult<(), Error> {
        self.inner.new_parent_view(height, maybe_payload)
    }

    /// Returns the number of blocks cached.
    pub fn cached_blocks(&self) -> Stm<BlockHeight> {
        self.inner.cached_blocks()
    }

    pub fn first_non_null_block(&self, height: BlockHeight) -> Stm<Option<BlockHeight>> {
        self.inner.first_non_null_block(height)
    }
}

#[cfg(test)]
mod tests {
    use crate::finality::ParentViewPayload;
    use crate::proxy::ParentQueryProxy;
    use crate::{
        BlockHeight, CachedFinalityProvider, Config, IPCParentFinality, ParentViewProvider,
        SequentialKeyCache, NULL_ROUND_ERR_MSG,
    };
    use anyhow::anyhow;
    use async_trait::async_trait;
    use fvm_shared::address::Address;
    use fvm_shared::econ::TokenAmount;
    use ipc_api::cross::IpcEnvelope;
    use ipc_api::staking::{StakingChange, StakingChangeRequest, StakingOperation};
    use ipc_api::subnet_id::SubnetID;
    use ipc_provider::manager::{GetBlockHashResult, TopDownQueryPayload};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Creates a mock of a new parent blockchain view. The key is the height and the value is the
    /// block hash. If block hash is None, it means the current height is a null block.
    macro_rules! new_parent_blocks {
        ($($key:expr => $val:expr),* ,) => (
            hash_map!($($key => $val),*)
        );
        ($($key:expr => $val:expr),*) => ({
            let mut map = SequentialKeyCache::sequential();
            $( map.append($key, $val).unwrap(); )*
            map
        });
    }

    struct TestParentProxy {
        blocks: SequentialKeyCache<BlockHeight, Option<ParentViewPayload>>,
    }

    #[async_trait]
    impl ParentQueryProxy for TestParentProxy {
        async fn get_chain_head_height(&self) -> anyhow::Result<BlockHeight> {
            Ok(self.blocks.upper_bound().unwrap())
        }

        async fn get_genesis_epoch(&self) -> anyhow::Result<BlockHeight> {
            Ok(self.blocks.lower_bound().unwrap() - 1)
        }

        async fn get_block_hash(&self, height: BlockHeight) -> anyhow::Result<GetBlockHashResult> {
            let r = self.blocks.get_value(height).unwrap();
            if r.is_none() {
                return Err(anyhow!(NULL_ROUND_ERR_MSG));
            }

            for h in (self.blocks.lower_bound().unwrap()..height).rev() {
                let v = self.blocks.get_value(h).unwrap();
                if v.is_none() {
                    continue;
                }
                return Ok(GetBlockHashResult {
                    parent_block_hash: v.clone().unwrap().0,
                    block_hash: r.clone().unwrap().0,
                });
            }
            panic!("invalid testing data")
        }

        async fn get_top_down_msgs(
            &self,
            height: BlockHeight,
        ) -> anyhow::Result<TopDownQueryPayload<Vec<IpcEnvelope>>> {
            let r = self.blocks.get_value(height).cloned().unwrap();
            if r.is_none() {
                return Err(anyhow!(NULL_ROUND_ERR_MSG));
            }
            let r = r.unwrap();
            Ok(TopDownQueryPayload {
                value: r.2,
                block_hash: r.0,
            })
        }

        async fn get_validator_changes(
            &self,
            height: BlockHeight,
        ) -> anyhow::Result<TopDownQueryPayload<Vec<StakingChangeRequest>>> {
            let r = self.blocks.get_value(height).cloned().unwrap();
            if r.is_none() {
                return Err(anyhow!(NULL_ROUND_ERR_MSG));
            }
            let r = r.unwrap();
            Ok(TopDownQueryPayload {
                value: r.1,
                block_hash: r.0,
            })
        }
    }

    fn new_provider(
        blocks: SequentialKeyCache<BlockHeight, Option<ParentViewPayload>>,
    ) -> CachedFinalityProvider<TestParentProxy> {
        let config = Config {
            chain_head_delay: 2,
            polling_interval: Default::default(),
            exponential_back_off: Default::default(),
            exponential_retry_limit: 0,
            max_proposal_range: Some(1),
            max_cache_blocks: None,
            proposal_delay: None,
        };
        let genesis_epoch = blocks.lower_bound().unwrap();
        let proxy = Arc::new(TestParentProxy { blocks });
        let committed_finality = IPCParentFinality {
            height: genesis_epoch,
            block_hash: vec![0; 32],
        };

        CachedFinalityProvider::new(config, genesis_epoch, Some(committed_finality), proxy)
    }

    fn new_cross_msg(nonce: u64) -> IpcEnvelope {
        let subnet_id = SubnetID::new(10, vec![Address::new_id(1000)]);
        let mut msg = IpcEnvelope::new_fund_msg(
            &subnet_id,
            &Address::new_id(1),
            &Address::new_id(2),
            TokenAmount::from_atto(100),
        )
        .unwrap();
        msg.nonce = nonce;
        msg
    }

    fn new_validator_changes(configuration_number: u64) -> StakingChangeRequest {
        StakingChangeRequest {
            configuration_number,
            change: StakingChange {
                op: StakingOperation::Deposit,
                payload: vec![],
                validator: Address::new_id(1),
            },
        }
    }

    #[tokio::test]
    async fn test_retry() {
        struct Test {
            nums_run: AtomicUsize,
        }

        impl Test {
            async fn run(&self) -> Result<(), &'static str> {
                self.nums_run.fetch_add(1, Ordering::SeqCst);
                Err("mocked error")
            }
        }

        let t = Test {
            nums_run: AtomicUsize::new(0),
        };

        let res = retry!(Duration::from_secs(1), 2, t.run().await);
        assert!(res.is_err());
        // execute the first time, retries twice
        assert_eq!(t.nums_run.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_query_topdown_msgs() {
        let parent_blocks = new_parent_blocks!(
            100 => Some((vec![0; 32], vec![], vec![new_cross_msg(0)])),   // genesis block
            101 => Some((vec![1; 32], vec![], vec![new_cross_msg(1)])),
            102 => Some((vec![2; 32], vec![], vec![new_cross_msg(2)])),
            103 => Some((vec![3; 32], vec![], vec![new_cross_msg(3)])),
            104 => None,
            105 => None,
            106 => Some((vec![6; 32], vec![], vec![new_cross_msg(6)]))
        );
        let provider = new_provider(parent_blocks);
        let messages = provider.top_down_msgs_from(100, 106).await.unwrap();

        assert_eq!(
            messages,
            vec![
                new_cross_msg(0),
                new_cross_msg(1),
                new_cross_msg(2),
                new_cross_msg(3),
                new_cross_msg(6),
            ]
        )
    }

    #[tokio::test]
    async fn test_query_validator_changes() {
        let parent_blocks = new_parent_blocks!(
            100 => Some((vec![0; 32], vec![new_validator_changes(0)], vec![])),   // genesis block
            101 => Some((vec![1; 32], vec![new_validator_changes(1)], vec![])),
            102 => Some((vec![2; 32], vec![], vec![])),
            103 => Some((vec![3; 32], vec![new_validator_changes(3)], vec![])),
            104 => None,
            105 => None,
            106 => Some((vec![6; 32], vec![new_validator_changes(6)], vec![]))
        );
        let provider = new_provider(parent_blocks);
        let messages = provider.validator_changes_from(100, 106).await.unwrap();

        assert_eq!(messages.len(), 4)
    }
}
