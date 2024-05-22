// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use async_trait::async_trait;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::{address::Address, econ::TokenAmount};
use ipc_api::checkpoint::{
    BottomUpCheckpoint, BottomUpCheckpointBundle, QuorumReachedEvent, Signature,
};
use ipc_api::cross::IpcEnvelope;
use ipc_api::staking::{StakingChangeRequest, ValidatorInfo};
use ipc_api::subnet::{ConstructParams, PermissionMode, SupplySource};
use ipc_api::subnet_id::SubnetID;
use ipc_api::validator::Validator;

use crate::lotus::message::ipc::SubnetInfo;

/// Trait to interact with a subnet and handle its lifecycle.
#[async_trait]
pub trait SubnetManager: Send + Sync + TopDownFinalityQuery + BottomUpCheckpointRelayer {
    /// Deploys a new subnet actor on the `parent` subnet and with the
    /// configuration passed in `ConstructParams`.
    /// The result of the function is the ID address for the subnet actor from which the final
    /// subet ID can be inferred.
    async fn create_subnet(&self, from: Address, params: ConstructParams) -> Result<Address>;

    /// Performs the call to join a subnet from a wallet address and staking an amount
    /// of collateral. This function, as well as all of the ones on this trait, can infer
    /// the specific subnet and actors on which to perform the relevant calls from the
    /// SubnetID given as an argument.
    async fn join_subnet(
        &self,
        subnet: SubnetID,
        from: Address,
        collateral: TokenAmount,
        metadata: Vec<u8>,
    ) -> Result<ChainEpoch>;

    /// Adds some initial balance to an address before a child subnet bootstraps to make
    /// it available in the subnet at genesis.
    async fn pre_fund(&self, subnet: SubnetID, from: Address, balance: TokenAmount) -> Result<()>;

    /// Releases initial funds from an address for a subnet that has not yet been bootstrapped
    async fn pre_release(&self, subnet: SubnetID, from: Address, amount: TokenAmount)
        -> Result<()>;

    /// Allows validators that have already joined the subnet to stake more collateral
    /// and increase their power in the subnet.
    async fn stake(&self, subnet: SubnetID, from: Address, collateral: TokenAmount) -> Result<()>;

    /// Allows validators that have already joined the subnet to unstake collateral
    /// and reduce their power in the subnet.
    async fn unstake(&self, subnet: SubnetID, from: Address, collateral: TokenAmount)
        -> Result<()>;

    /// Sends a request to leave a subnet from a wallet address.
    async fn leave_subnet(&self, subnet: SubnetID, from: Address) -> Result<()>;

    /// Sends a signal to kill a subnet
    async fn kill_subnet(&self, subnet: SubnetID, from: Address) -> Result<()>;

    /// Lists all the registered children in a gateway.
    async fn list_child_subnets(
        &self,
        gateway_addr: Address,
    ) -> Result<HashMap<SubnetID, SubnetInfo>>;

    /// Claims any collateral that may be available to claim by validators that
    /// have left the subnet.
    async fn claim_collateral(&self, subnet: SubnetID, from: Address) -> Result<()>;

    /// Fund injects new funds from an account of the parent chain to a subnet.
    /// Returns the epoch that the fund is executed in the parent.
    async fn fund(
        &self,
        subnet: SubnetID,
        gateway_addr: Address,
        from: Address,
        to: Address,
        amount: TokenAmount,
    ) -> Result<ChainEpoch>;

    /// Sends funds to a specified subnet receiver using ERC20 tokens.
    /// This function locks the amount of ERC20 tokens into custody and then mints the supply in the specified subnet.
    /// It checks if the subnet's supply strategy is ERC20 and if not, the operation is reverted.
    /// It allows for free injection of funds into a subnet and is protected against reentrancy.
    ///
    /// # Arguments
    ///
    /// * `subnetId` - The ID of the subnet where the funds will be sent to.
    /// * `from`     - The funding address.
    /// * `to`       - The funded address.
    /// * `amount`   - The amount of ERC20 tokens to be sent.
    async fn fund_with_token(
        &self,
        subnet: SubnetID,
        from: Address,
        to: Address,
        amount: TokenAmount,
    ) -> Result<ChainEpoch>;

    /// Grants an allowance to the `from` address to withdraw up to `amount` of tokens from the contract at `token_address`.
    /// This function sets up an approval, allowing the `from` address to later transfer or utilize the tokens from the specified ERC20 token contract.
    /// The primary use case is to enable subsequent contract interactions that require an upfront allowance,
    /// such as depositing tokens into a contract that requires an allowance check.
    ///
    /// The operation ensures that the caller has the necessary authority and token balance before setting the allowance.
    /// It is crucial for enabling controlled access to the token funds without transferring the ownership.
    /// Note that calling this function multiple times can overwrite the existing allowance with the new value.
    ///
    /// # Arguments
    ///
    /// * `from`         - The address granting the approval.
    /// * `token_address`- The contract address of the ERC20 token for which the approval is being granted.
    /// * `amount`       - The maximum amount of tokens `from` is allowing to be used.
    ///
    /// # Returns
    ///
    /// * `Result<()>`   - An empty result indicating success or an error on failure, encapsulating any issues encountered during the approval process.
    async fn approve_token(
        &self,
        subnet: SubnetID,
        from: Address,
        amount: TokenAmount,
    ) -> Result<ChainEpoch>;

    /// Release creates a new check message to release funds in parent chain
    /// Returns the epoch that the released is executed in the child.
    async fn release(
        &self,
        gateway_addr: Address,
        from: Address,
        to: Address,
        amount: TokenAmount,
    ) -> Result<ChainEpoch>;

    /// Propagate a cross-net message forward. For `postbox_msg_key`, we are using bytes because different
    /// runtime have different representations. For FVM, it should be `CID` as bytes. For EVM, it is
    /// `bytes32`.
    async fn propagate(
        &self,
        subnet: SubnetID,
        gateway_addr: Address,
        from: Address,
        postbox_msg_key: Vec<u8>,
    ) -> Result<()>;

    /// Send value between two addresses in a subnet
    async fn send_value(&self, from: Address, to: Address, amount: TokenAmount) -> Result<()>;

    /// Get the balance of an address
    async fn wallet_balance(&self, address: &Address) -> Result<TokenAmount>;

    /// Get chainID for the network.
    /// Returning as a `String` because the maximum value for an EVM
    /// networks is a `U256` that wouldn't fit in an integer type.
    async fn get_chain_id(&self) -> Result<String>;

    /// Get commit sha for deployed contracts
    async fn get_commit_sha(&self) -> Result<[u8; 32]>;

    /// Gets the subnet supply source
    async fn get_subnet_supply_source(
        &self,
        subnet: &SubnetID,
    ) -> Result<ipc_actors_abis::subnet_actor_getter_facet::SupplySource>;

    /// Gets the genesis information required to bootstrap a child subnet
    async fn get_genesis_info(&self, subnet: &SubnetID) -> Result<SubnetGenesisInfo>;

    /// Advertises the endpoint of a bootstrap node for the subnet.
    async fn add_bootstrap(
        &self,
        subnet: &SubnetID,
        from: &Address,
        endpoint: String,
    ) -> Result<()>;

    /// Lists the bootstrap nodes of a subnet
    async fn list_bootstrap_nodes(&self, subnet: &SubnetID) -> Result<Vec<String>>;

    /// Get the validator information
    async fn get_validator_info(
        &self,
        subnet: &SubnetID,
        validator: &Address,
    ) -> Result<ValidatorInfo>;

    async fn set_federated_power(
        &self,
        from: &Address,
        subnet: &SubnetID,
        validators: &[Address],
        public_keys: &[Vec<u8>],
        federated_power: &[u128],
    ) -> Result<ChainEpoch>;
}

#[derive(Debug)]
pub struct SubnetGenesisInfo {
    pub bottom_up_checkpoint_period: u64,
    pub majority_percentage: u8,
    pub active_validators_limit: u16,
    pub min_collateral: TokenAmount,
    pub genesis_epoch: ChainEpoch,
    pub validators: Vec<Validator>,
    pub genesis_balances: BTreeMap<Address, TokenAmount>,
    pub permission_mode: PermissionMode,
    pub supply_source: SupplySource,
}

/// The generic payload that returns the block hash of the data returning block with the actual
/// data payload.
#[derive(Debug)]
pub struct TopDownQueryPayload<T> {
    pub value: T,
    pub block_hash: Vec<u8>,
}

#[derive(Default, Debug)]
pub struct GetBlockHashResult {
    pub parent_block_hash: Vec<u8>,
    pub block_hash: Vec<u8>,
}

/// Trait to interact with a subnet to query the necessary information for top down checkpoint.
#[async_trait]
pub trait TopDownFinalityQuery: Send + Sync {
    /// Returns the genesis epoch that the subnet is created in parent network
    async fn genesis_epoch(&self, subnet_id: &SubnetID) -> Result<ChainEpoch>;
    /// Returns the chain head height
    async fn chain_head_height(&self) -> Result<ChainEpoch>;
    /// Returns the list of top down messages
    async fn get_top_down_msgs(
        &self,
        subnet_id: &SubnetID,
        epoch: ChainEpoch,
    ) -> Result<TopDownQueryPayload<Vec<IpcEnvelope>>>;
    /// Get the block hash
    async fn get_block_hash(&self, height: ChainEpoch) -> Result<GetBlockHashResult>;
    /// Get the validator change set from start to end block.
    async fn get_validator_changeset(
        &self,
        subnet_id: &SubnetID,
        epoch: ChainEpoch,
    ) -> Result<TopDownQueryPayload<Vec<StakingChangeRequest>>>;
    /// Returns the latest parent finality committed in a child subnet
    async fn latest_parent_finality(&self) -> Result<ChainEpoch>;
}

/// The bottom up checkpoint manager that handles the bottom up relaying from child subnet to the parent
/// subnet.
#[async_trait]
pub trait BottomUpCheckpointRelayer: Send + Sync {
    /// Submit a checkpoint for execution.
    /// It triggers the commitment of the checkpoint and the execution of related cross-net messages.
    /// Returns the epoch that the execution is successful
    async fn submit_checkpoint(
        &self,
        submitter: &Address,
        checkpoint: BottomUpCheckpoint,
        signatures: Vec<Signature>,
        signatories: Vec<Address>,
    ) -> Result<ChainEpoch>;
    /// The last confirmed/submitted checkpoint height.
    async fn last_bottom_up_checkpoint_height(&self, subnet_id: &SubnetID) -> Result<ChainEpoch>;
    /// Get the checkpoint period, i.e the number of blocks to submit bottom up checkpoints.
    async fn checkpoint_period(&self, subnet_id: &SubnetID) -> Result<ChainEpoch>;
    /// Get the checkpoint bundle at a specific height. If it does not exist, it will through error.
    async fn checkpoint_bundle_at(
        &self,
        height: ChainEpoch,
    ) -> Result<Option<BottomUpCheckpointBundle>>;
    /// Queries the signature quorum reached events at target height.
    async fn quorum_reached_events(&self, height: ChainEpoch) -> Result<Vec<QuorumReachedEvent>>;
    /// Get the current epoch in the current subnet
    async fn current_epoch(&self) -> Result<ChainEpoch>;
}
