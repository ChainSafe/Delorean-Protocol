// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

mod fetch;
mod null;

use crate::error::Error;
use crate::BlockHash;
use async_stm::{abort, StmResult};
use ipc_api::cross::IpcEnvelope;
use ipc_api::staking::StakingChangeRequest;

pub use fetch::CachedFinalityProvider;

pub(crate) type ParentViewPayload = (BlockHash, Vec<StakingChangeRequest>, Vec<IpcEnvelope>);

fn ensure_sequential<T, F: Fn(&T) -> u64>(msgs: &[T], f: F) -> StmResult<(), Error> {
    if msgs.is_empty() {
        return Ok(());
    }

    let first = msgs.first().unwrap();
    let mut nonce = f(first);
    for msg in msgs.iter().skip(1) {
        if nonce + 1 != f(msg) {
            return abort(Error::NotSequential);
        }
        nonce += 1;
    }

    Ok(())
}

pub(crate) fn validator_changes(p: &ParentViewPayload) -> Vec<StakingChangeRequest> {
    p.1.clone()
}

pub(crate) fn topdown_cross_msgs(p: &ParentViewPayload) -> Vec<IpcEnvelope> {
    p.2.clone()
}

#[cfg(test)]
mod tests {
    use crate::proxy::ParentQueryProxy;
    use crate::{
        BlockHeight, CachedFinalityProvider, Config, IPCParentFinality, ParentFinalityProvider,
    };
    use async_stm::atomically_or_err;
    use async_trait::async_trait;
    use ipc_api::cross::IpcEnvelope;
    use ipc_api::staking::StakingChangeRequest;
    use ipc_provider::manager::{GetBlockHashResult, TopDownQueryPayload};
    use std::sync::Arc;
    use tokio::time::Duration;

    struct MockedParentQuery;

    #[async_trait]
    impl ParentQueryProxy for MockedParentQuery {
        async fn get_chain_head_height(&self) -> anyhow::Result<BlockHeight> {
            Ok(1)
        }

        async fn get_genesis_epoch(&self) -> anyhow::Result<BlockHeight> {
            Ok(10)
        }

        async fn get_block_hash(&self, _height: BlockHeight) -> anyhow::Result<GetBlockHashResult> {
            Ok(GetBlockHashResult::default())
        }

        async fn get_top_down_msgs(
            &self,
            _height: BlockHeight,
        ) -> anyhow::Result<TopDownQueryPayload<Vec<IpcEnvelope>>> {
            Ok(TopDownQueryPayload {
                value: vec![],
                block_hash: vec![],
            })
        }

        async fn get_validator_changes(
            &self,
            _height: BlockHeight,
        ) -> anyhow::Result<TopDownQueryPayload<Vec<StakingChangeRequest>>> {
            Ok(TopDownQueryPayload {
                value: vec![],
                block_hash: vec![],
            })
        }
    }

    fn mocked_agent_proxy() -> Arc<MockedParentQuery> {
        Arc::new(MockedParentQuery)
    }

    fn genesis_finality() -> IPCParentFinality {
        IPCParentFinality {
            height: 0,
            block_hash: vec![0; 32],
        }
    }

    fn new_provider() -> CachedFinalityProvider<MockedParentQuery> {
        let config = Config {
            chain_head_delay: 20,
            polling_interval: Duration::from_secs(10),
            exponential_back_off: Duration::from_secs(10),
            exponential_retry_limit: 10,
            max_proposal_range: None,
            max_cache_blocks: None,
            proposal_delay: None,
        };

        CachedFinalityProvider::new(config, 10, Some(genesis_finality()), mocked_agent_proxy())
    }

    #[tokio::test]
    async fn test_finality_works() {
        let provider = new_provider();

        atomically_or_err(|| {
            // inject data
            for i in 10..=100 {
                provider.new_parent_view(i, Some((vec![1u8; 32], vec![], vec![])))?;
            }

            let target_block = 120;
            let finality = IPCParentFinality {
                height: target_block,
                block_hash: vec![1u8; 32],
            };
            provider.set_new_finality(finality.clone(), Some(genesis_finality()))?;

            // all cache should be cleared
            let r = provider.next_proposal()?;
            assert!(r.is_none());

            let f = provider.last_committed_finality()?;
            assert_eq!(f, Some(finality));

            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_check_proposal_works() {
        let provider = new_provider();

        atomically_or_err(|| {
            let target_block = 100;

            // inject data
            provider.new_parent_view(target_block, Some((vec![1u8; 32], vec![], vec![])))?;
            provider.set_new_finality(
                IPCParentFinality {
                    height: target_block - 1,
                    block_hash: vec![1u8; 32],
                },
                Some(genesis_finality()),
            )?;

            let finality = IPCParentFinality {
                height: target_block,
                block_hash: vec![1u8; 32],
            };

            assert!(provider.check_proposal(&finality).is_ok());

            Ok(())
        })
        .await
        .unwrap();
    }
}
