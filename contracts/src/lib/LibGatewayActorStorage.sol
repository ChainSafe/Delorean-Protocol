// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {NotSystemActor, NotEnoughFunds} from "../errors/IPCErrors.sol";
import {QuorumMap} from "../structs/Quorum.sol";
import {BottomUpCheckpoint, BottomUpMsgBatch, IpcEnvelope, ParentFinality} from "../structs/CrossNet.sol";
import {SubnetID, Subnet, ParentValidatorsTracker} from "../structs/Subnet.sol";
import {Membership} from "../structs/Subnet.sol";
import {AccountHelper} from "../lib/AccountHelper.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

struct GatewayActorStorage {
    /// @notice The latest parent height committed.
    uint256 latestParentHeight;
    /// @notice bottom-up period in number of epochs for the subnet
    uint256 bottomUpCheckPeriod;
    /// @notice bottom-up message batch period in number of epochs for the subnet
    uint256 bottomUpMsgBatchPeriod;
    /// @notice nonce for bottom-up messages
    uint64 bottomUpNonce;
    /// @notice AppliedNonces keep track of the next nonce of the message to be applied.
    /// This prevents potential replay attacks.
    uint64 appliedTopDownNonce;
    /// @notice Number of active subnets spawned from this one
    uint64 totalSubnets;
    /// @notice Maximum number of messages per batch
    uint64 maxMsgsPerBottomUpBatch;
    /// @notice majority percentage value (must be greater than or equal to 51)
    uint8 majorityPercentage;
    /// @notice Code commit SHA where this contract is from
    bytes32 commitSha;
    //
    // == Feature flags ==
    //
    /// @notice Determines the maximum depth that this instance of the gateway
    /// will enforce. Bear in mind that the deployment is decentralized,
    /// and a subnet could choose not to change this code and not enforce
    /// this as a maximum depth in its own subnet.
    uint8 maxTreeDepth;
    /// @notice Determines if general purpose cros-net messages are supported
    bool generalPurposeCrossMsg;
    /// @notice Determines if multi-level cross-net messages are enbaled.
    bool multiLevelCrossMsg;
    // == Structs ==
    /// @notice The current membership of the child subnet
    Membership currentMembership;
    /// @notice The last membership received from the parent and adopted
    Membership lastMembership;
    /// @notice Quorum information for checkpoints
    QuorumMap checkpointQuorumMap;
    /// @notice path to the current network
    SubnetID networkName;
    /// Tracking validator changes from parent in child subnet
    ParentValidatorsTracker validatorsTracker;
    //
    // == Dynamic types ==
    //
    /// @notice List of subnets
    /// SubnetID => Subnet
    mapping(bytes32 => Subnet) subnets;
    /// @notice The parent finalities. Key is the block number, value is the finality struct.
    mapping(uint256 => ParentFinality) finalitiesMap;
    /// @notice Postbox keeps track of all the cross-net messages triggered by
    /// an actor that need to be propagated further through the hierarchy.
    /// cross-net message id => CrossMsg
    mapping(bytes32 => IpcEnvelope) postbox;
    /// @notice A mapping of block numbers to bottom-up checkpoints
    // slither-disable-next-line uninitialized-state
    mapping(uint256 => BottomUpCheckpoint) bottomUpCheckpoints;
    /// @notice A mapping of block numbers to bottom-up cross-messages
    // slither-disable-next-line uninitialized-state
    mapping(uint256 => BottomUpMsgBatch) bottomUpMsgBatches;
    /// @notice Keys of the registered subnets. Useful to iterate through them
    EnumerableSet.Bytes32Set subnetKeys;
}

library LibGatewayActorStorage {
    function appStorage() internal pure returns (GatewayActorStorage storage ds) {
        assembly {
            ds.slot := 0
        }
        return ds;
    }
}

contract GatewayActorModifiers {
    GatewayActorStorage internal s;

    using FilAddress for address;
    using FilAddress for address payable;
    using AccountHelper for address;

    function _systemActorOnly() private view {
        if (!msg.sender.isSystemActor()) {
            revert NotSystemActor();
        }
    }

    modifier systemActorOnly() {
        _systemActorOnly();
        _;
    }
}
