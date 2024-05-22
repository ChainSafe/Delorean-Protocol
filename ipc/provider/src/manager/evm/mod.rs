// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

mod manager;

use async_trait::async_trait;
use fvm_shared::clock::ChainEpoch;
use ipc_api::subnet_id::SubnetID;

use super::subnet::SubnetManager;
pub use manager::EthSubnetManager;

use ipc_actors_abis::subnet_actor_checkpointing_facet;

#[async_trait]
pub trait EthManager: SubnetManager {
    /// The current epoch/block number of the blockchain that the manager connects to.
    async fn current_epoch(&self) -> anyhow::Result<ChainEpoch>;

    /// Get all the top down messages till a certain epoch
    async fn bottom_up_checkpoint(
        &self,
        epoch: ChainEpoch,
    ) -> anyhow::Result<subnet_actor_checkpointing_facet::BottomUpCheckpoint>;

    /// Get the latest applied top down nonce
    async fn get_applied_top_down_nonce(&self, subnet_id: &SubnetID) -> anyhow::Result<u64>;

    /// Get the subnet contract bottom up checkpoint period
    async fn subnet_bottom_up_checkpoint_period(
        &self,
        subnet_id: &SubnetID,
    ) -> anyhow::Result<ChainEpoch>;

    /// Get the previous checkpoint hash from the gateway
    async fn prev_bottom_up_checkpoint_hash(
        &self,
        subnet_id: &SubnetID,
        epoch: ChainEpoch,
    ) -> anyhow::Result<[u8; 32]>;

    /// The minimal number of validators required for the subnet
    async fn min_validators(&self, subnet_id: &SubnetID) -> anyhow::Result<u64>;
}
