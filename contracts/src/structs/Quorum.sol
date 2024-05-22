// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

/// @notice A kind of quorum.
enum QuorumObjKind {
    Checkpoint,
    BottomUpMsgBatch
}

/// @notice Checkpoint quorum information.
struct QuorumInfo {
    /// @dev The hash of the corresponding bottom-up checkpoint.
    bytes32 hash;
    /// @dev The root hash of the Merkle tree built from the validator public keys and their weight.
    bytes32 rootHash;
    /// @dev The target weight that must be reached to accept the checkpoint.
    uint256 threshold;
    /// @dev The current weight of the checkpoint.
    uint256 currentWeight;
    /// @dev Whether the quorum has already been reached.
    bool reached;
}

/// @notice A type aggregating quorum related information.
struct QuorumMap {
    /// @notice flags the type of object we are running a quorum over.
    QuorumObjKind quorumObjKind;
    /// @notice The height of the first bottom-up checkpoint that must be retained since they have not been processed in the parent.
    /// All checkpoint with the height less than this number may be garbage collected in the child subnet.
    /// @dev Initial retention index is 1.
    uint256 retentionHeight;
    /// @notice A mapping of block numbers to quorum info
    mapping(uint256 => QuorumInfo) quorumInfo;
    /// @notice A list of incomplete checkpoints.
    // slither-disable-next-line uninitialized-state
    EnumerableSet.UintSet incompleteQuorums;
    /// @notice The addresses of the validators that have already sent signatures at height `h`
    mapping(uint256 => EnumerableSet.AddressSet) quorumSignatureSenders;
    /// @notice The list of the collected signatures at height `h`
    mapping(uint256 => mapping(address => bytes)) quorumSignatures;
}
