// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {BottomUpCheckpoint, BottomUpMsgBatch, IpcEnvelope, ParentFinality} from "../structs/CrossNet.sol";
import {SubnetID} from "../structs/Subnet.sol";
import {FvmAddress} from "../structs/FvmAddress.sol";

/// @title Gateway interface
/// @author LimeChain team
interface IGateway {
    /// @notice Register is called by subnet actors to put the required collateral
    /// and register the subnet to the hierarchy.
    function register(uint256 genesisCircSupply) external payable;

    /// @notice AddStake adds stake to the collateral of a subnet.
    function addStake() external payable;

    /// @notice Release stake recovers some collateral of the subnet
    function releaseStake(uint256 amount) external;

    /// @notice Kill propagates the kill signal from a subnet actor to unregister it from th
    /// hierarchy.
    function kill() external;

    /// @notice commitCheckpoint propagates the commitment of a checkpoint from a child
    function commitCheckpoint(BottomUpCheckpoint calldata bottomUpCheckpoint) external;

    /// @notice fund locks the received funds —denominated in the native coin— and moves the value down the hierarchy,
    /// crediting the funds to the specified address in the destination network.
    ///
    /// This functions ends up minting supply in the subnet equal to the value of the transaction. It does so by
    /// committing the relevant top-down message, updating the top-down nonce along the way.
    ///
    /// Calling this method on a subnet whose supply source is not 'native' will revert with UnexpectedSupplySource().
    function fund(SubnetID calldata subnetId, FvmAddress calldata to) external payable;

    /// @notice fundWithToken locks the specified amount of tokens in the ERC20 contract linked to the subnet, and
    /// moves the value down the hierarchy, crediting the funds as native coins to the specified address
    /// in the destination network.
    ///
    /// This method expects the caller to have approved the gateway to spend `amount` tokens on their behalf
    /// (usually done through IERC20#approve). Tokens are locked by calling IERC20#transferFrom(caller, address(this), amount).
    /// A failure in transferring tokens to the gateway will revert the call.
    ///
    /// It's possible to call this method from an EOA or a contract. Regardless, it's recommended to approve strictly
    /// the amount that will subsequently be deposited into the subnet. Keeping outstanding approvals is not recommended.
    ///
    /// Calling this method on a subnet whose supply source is not 'ERC20' will revert with UnexpectedSupplySource().
    function fundWithToken(SubnetID calldata subnetId, FvmAddress calldata to, uint256 amount) external;

    /// @notice Release creates a new check message to release funds in parent chain
    ///
    /// This function burns the funds that will be released in the current subnet
    /// and propagates a new checkpoint message to the parent chain to signal
    /// the amount of funds that can be released for a specific address.
    function release(FvmAddress calldata to) external payable;

    /// @notice sendContractXnetMessage sends an arbitrary cross-message to other subnet in the hierarchy.
    // TODO: add the right comment and function name here.
    function sendContractXnetMessage(
        IpcEnvelope calldata envelope
    ) external payable returns (IpcEnvelope memory committed);

    /// @notice Propagates the stored postbox item for the given cid
    function propagate(bytes32 msgCid) external payable;

    /// @notice commit the ipc parent finality into storage
    function commitParentFinality(ParentFinality calldata finality) external;

    /// @notice creates a new bottom-up checkpoint
    function createBottomUpCheckpoint(
        BottomUpCheckpoint calldata checkpoint,
        bytes32 membershipRootHash,
        uint256 membershipWeight
    ) external;
}
