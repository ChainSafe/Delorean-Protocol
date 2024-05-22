// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {BottomUpCheckpoint, BottomUpMsgBatch, IpcEnvelope, ParentFinality} from "../structs/CrossNet.sol";
import {QuorumInfo} from "../structs/Quorum.sol";
import {SubnetID, Subnet} from "../structs/Subnet.sol";
import {Membership} from "../structs/Subnet.sol";
import {LibGateway} from "../lib/LibGateway.sol";
import {LibStaking} from "../lib/LibStaking.sol";
import {LibQuorum} from "../lib/LibQuorum.sol";
import {GatewayActorStorage} from "../lib/LibGatewayActorStorage.sol";
import {SubnetIDHelper} from "../lib/SubnetIDHelper.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

contract GatewayGetterFacet {
    // slither-disable-next-line uninitialized-state
    GatewayActorStorage internal s;

    using SubnetIDHelper for SubnetID;
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;
    using EnumerableSet for EnumerableSet.Bytes32Set;

    /// @notice Returns the next and start configuration numbers in the validator changes.
    /// The configuration numbers are from changes made in the parent.
    function getValidatorConfigurationNumbers() external view returns (uint64, uint64) {
        return LibStaking.getConfigurationNumbers();
    }

    /// @notice Returns code commit SHA where this contract is from.
    function getCommitSha() external view returns (bytes32) {
        return s.commitSha;
    }

    /// @notice Returns the current nonce for bottom-up message processing.
    function bottomUpNonce() external view returns (uint64) {
        return s.bottomUpNonce;
    }

    /// @notice Returns the total number of the registered subnets.
    function totalSubnets() external view returns (uint64) {
        return s.totalSubnets;
    }

    /// @notice Returns the maximum number of messages per bottom-up batch.
    function maxMsgsPerBottomUpBatch() external view returns (uint64) {
        return s.maxMsgsPerBottomUpBatch;
    }

    /// @notice Returns the period for bottom-up checkpointing.
    function bottomUpCheckPeriod() external view returns (uint256) {
        return s.bottomUpCheckPeriod;
    }

    /// @notice Returns the subnet identifier of the network.
    function getNetworkName() external view returns (SubnetID memory) {
        return s.networkName;
    }

    /// @notice Returns a specific bottom-up checkpoint based on an epoch number.
    /// @param e The epoch number of the checkpoint.
    function bottomUpCheckpoint(uint256 e) external view returns (BottomUpCheckpoint memory) {
        return s.bottomUpCheckpoints[e];
    }

    /// @notice Returns a specific bottom-up message batch based on an index.
    /// @param e The epoch number of the batch.
    function bottomUpMsgBatch(uint256 e) external view returns (BottomUpMsgBatch memory) {
        return s.bottomUpMsgBatches[e];
    }

    /// @notice Returns the parent chain finality information for a given block number.
    /// @param blockNumber The block number for which to retrieve parent-finality information.
    function getParentFinality(uint256 blockNumber) external view returns (ParentFinality memory) {
        return LibGateway.getParentFinality(blockNumber);
    }

    /// @notice Gets the most recent parent-finality information from the parent.
    function getLatestParentFinality() external view returns (ParentFinality memory) {
        return LibGateway.getLatestParentFinality();
    }

    /// @notice Returns the subnet with the given id.
    /// @param subnetId the id of the subnet.
    /// @return found whether the subnet exists.
    /// @return subnet -  the subnet struct.
    function getSubnet(SubnetID calldata subnetId) external view returns (bool, Subnet memory) {
        // slither-disable-next-line unused-return
        return LibGateway.getSubnet(subnetId);
    }

    /// @notice Returns information about a specific subnet using its hash identifier.
    /// @param h The hash identifier of the subnet to be queried.
    /// @return subnet The subnet information corresponding to the given hash.
    function subnets(bytes32 h) external view returns (Subnet memory subnet) {
        return s.subnets[h];
    }

    /// @notice Returns the length of the top-down message queue for a specified subnet.
    /// @param subnetId The identifier of the subnet for which the message queue length is queried.
    /// @return The current length of the top-down message queue, indicated by the subnet's top-down nonce.
    function getSubnetTopDownMsgsLength(SubnetID memory subnetId) external view returns (uint256) {
        // slither-disable-next-line unused-return
        (, Subnet storage subnet) = LibGateway.getSubnet(subnetId);
        // With every new message, the nonce is added by one, the current nonce should be equal to the top down message length.
        return subnet.topDownNonce;
    }

    /// @notice Returns the current applied top-down nonce for a specified subnet, indicating whether it's registered.
    /// @param subnetId The identifier of the subnet for which the top-down nonce is queried.
    /// @return A tuple containing a boolean indicating if the subnet is registered and the current top-down nonce.
    function getTopDownNonce(SubnetID calldata subnetId) external view returns (bool, uint64) {
        (bool registered, Subnet storage subnet) = LibGateway.getSubnet(subnetId);
        if (!registered) {
            return (false, 0);
        }
        return (true, subnet.topDownNonce);
    }

    /// @notice Returns the current applied bottom-up nonce for a specified subnet, indicating whether it's registered.
    /// @param subnetId The identifier of the subnet for which the bottom-up nonce is queried.
    /// @return A tuple containing a boolean indicating if the subnet is registered and the current applied bottom-up nonce.
    function getAppliedBottomUpNonce(SubnetID calldata subnetId) external view returns (bool, uint64) {
        (bool registered, Subnet storage subnet) = LibGateway.getSubnet(subnetId);
        if (!registered) {
            return (false, 0);
        }
        return (true, subnet.appliedBottomUpNonce);
    }

    /// @notice Returns the current applied top-down nonce of the gateway.
    function appliedTopDownNonce() external view returns (uint64) {
        return s.appliedTopDownNonce;
    }

    /// @notice Returns the storable message and its wrapped status from the postbox by a given identifier.
    /// @param id The unique identifier of the message in the postbox.
    function postbox(bytes32 id) external view returns (IpcEnvelope memory storableMsg) {
        return (s.postbox[id]);
    }

    /// @notice Returns the majority percentage required for certain consensus or decision-making processes.
    function majorityPercentage() external view returns (uint64) {
        return s.majorityPercentage;
    }

    /// @notice Returns the list of registered subnets.
    /// @return The list of the registered subnets.
    function listSubnets() external view returns (Subnet[] memory) {
        uint256 size = s.subnetKeys.length();
        Subnet[] memory out = new Subnet[](size);
        for (uint256 i; i < size; ) {
            bytes32 key = s.subnetKeys.at(i);
            out[i] = s.subnets[key];
            unchecked {
                ++i;
            }
        }
        return out;
    }

    /// @notice Returns the subnet keys.
    function getSubnetKeys() external view returns (bytes32[] memory) {
        return s.subnetKeys.values();
    }

    /// @notice Returns the last membership received from the parent.
    function getLastMembership() external view returns (Membership memory) {
        return s.lastMembership;
    }

    /// @notice Returns the last configuration number received from the parent.
    function getLastConfigurationNumber() external view returns (uint64) {
        return s.lastMembership.configurationNumber;
    }

    /// @notice Returns the current membership.
    function getCurrentMembership() external view returns (Membership memory) {
        return s.currentMembership;
    }

    /// @notice Returns the current configuration number.
    function getCurrentConfigurationNumber() external view returns (uint64) {
        return s.currentMembership.configurationNumber;
    }

    /// @notice Returns quorum information for a specific checkpoint based on its height.
    /// @param h The block height of the checkpoint.
    /// @return Quorum information associated with the given checkpoint height.
    function getCheckpointInfo(uint256 h) external view returns (QuorumInfo memory) {
        return s.checkpointQuorumMap.quorumInfo[h];
    }

    /// @notice Returns the checkpoint current weight corresponding to the block height.
    function getCheckpointCurrentWeight(uint256 h) external view returns (uint256) {
        return s.checkpointQuorumMap.quorumInfo[h].currentWeight;
    }

    /// @notice Returns the incomplete checkpoint heights.
    function getIncompleteCheckpointHeights() external view returns (uint256[] memory) {
        return s.checkpointQuorumMap.incompleteQuorums.values();
    }

    /// @notice Returns the incomplete checkpoints.
    function getIncompleteCheckpoints() external view returns (BottomUpCheckpoint[] memory) {
        uint256[] memory heights = s.checkpointQuorumMap.incompleteQuorums.values();
        uint256 size = heights.length;

        BottomUpCheckpoint[] memory checkpoints = new BottomUpCheckpoint[](size);
        for (uint64 i; i < size; ) {
            checkpoints[i] = s.bottomUpCheckpoints[heights[i]];
            unchecked {
                ++i;
            }
        }
        return checkpoints;
    }

    /// @notice Returns the bottom-up checkpoint retention index.
    function getCheckpointRetentionHeight() external view returns (uint256) {
        return s.checkpointQuorumMap.retentionHeight;
    }

    /// @notice Returns the threshold required for quorum in this subnet,
    ///         based on the configured majority percentage and the total weight of the validators.
    /// @param totalWeight The total weight to consider for calculating the quorum threshold.
    /// @return The quorum threshold derived from the total weight and majority percentage.
    function getQuorumThreshold(uint256 totalWeight) external view returns (uint256) {
        return LibQuorum.weightNeeded(totalWeight, s.majorityPercentage);
    }

    /// @notice Retrieves a bundle of information and signatures for a specified bottom-up checkpoint.
    /// @param h The height of the checkpoint for which information is requested.
    /// @return ch The checkpoint information at the specified height.
    /// @return info Quorum information related to the checkpoint.
    /// @return signatories An array of addresses of signatories who have signed the checkpoint.
    function getCheckpointSignatureBundle(
        uint256 h
    )
        external
        view
        returns (
            BottomUpCheckpoint memory ch,
            QuorumInfo memory info,
            address[] memory signatories,
            bytes[] memory signatures
        )
    {
        ch = s.bottomUpCheckpoints[h];
        (info, signatories, signatures) = LibQuorum.getSignatureBundle(s.checkpointQuorumMap, h);

        return (ch, info, signatories, signatures);
    }

    /// @notice Returns the current bottom-up checkpoint.
    /// @return exists - whether the checkpoint exists
    /// @return epoch - the epoch of the checkpoint
    /// @return checkpoint - the checkpoint struct
    function getCurrentBottomUpCheckpoint()
        external
        view
        returns (bool exists, uint256 epoch, BottomUpCheckpoint memory checkpoint)
    {
        (exists, epoch, checkpoint) = LibGateway.getCurrentBottomUpCheckpoint();
        return (exists, epoch, checkpoint);
    }
}
