// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {QuorumMap, QuorumInfo, QuorumObjKind} from "../structs/Quorum.sol";
import {InvalidRetentionHeight, QuorumAlreadyProcessed, FailedAddSignatory, InvalidSignature, SignatureReplay, NotAuthorized, FailedRemoveIncompleteQuorum, ZeroMembershipWeight, FailedAddIncompleteQuorum} from "../errors/IPCErrors.sol";
import {MerkleProof} from "openzeppelin-contracts/utils/cryptography/MerkleProof.sol";
import {ECDSA} from "openzeppelin-contracts/utils/cryptography/ECDSA.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

library LibQuorum {
    using EnumerableSet for EnumerableSet.UintSet;
    using EnumerableSet for EnumerableSet.AddressSet;

    event QuorumReached(QuorumObjKind objKind, uint256 height, bytes32 objHash, uint256 quorumWeight);
    event QuorumWeightUpdated(QuorumObjKind objKind, uint256 height, bytes32 objHash, uint256 newWeight);

    /// @notice checks whether the provided quorum signature for the block at height `height` is valid and accumulates that it
    /// @dev If adding the signature leads to reaching the threshold, then the info is removed from `incompleteQuorums`
    /// @param height - the height of the block in the checkpoint
    /// @param membershipProof - a Merkle proof that the validator was in the membership at height `height` with weight `weight`
    /// @param weight - the weight of the validator
    /// @param signature - the signature of the object we are agreen on
    function addQuorumSignature(
        QuorumMap storage self,
        uint256 height,
        bytes32[] memory membershipProof,
        uint256 weight,
        bytes memory signature
    ) internal {
        // get quorum info for height
        QuorumInfo storage info = self.quorumInfo[height];

        // slither-disable-next-line unused-return
        (address recoveredSignatory, ECDSA.RecoverError err, ) = ECDSA.tryRecover(info.hash, signature);
        if (err != ECDSA.RecoverError.NoError) {
            revert InvalidSignature();
        }

        // Check whether the validator has already sent a valid signature
        if (self.quorumSignatureSenders[height].contains(recoveredSignatory)) {
            revert SignatureReplay();
        }

        // The validator is allowed to send a signature if it was in the membership at the target height
        // Constructing leaf: https://github.com/OpenZeppelin/merkle-tree#leaf-hash
        bytes32 validatorLeaf = keccak256(bytes.concat(keccak256(abi.encode(recoveredSignatory, weight))));
        bool valid = MerkleProof.verify({proof: membershipProof, root: info.rootHash, leaf: validatorLeaf});
        if (!valid) {
            revert NotAuthorized(recoveredSignatory);
        }

        // All checks passed.
        // Adding signature and emitting events.

        bool ok = self.quorumSignatureSenders[height].add(recoveredSignatory);
        if (!ok) {
            revert FailedAddSignatory();
        }
        self.quorumSignatures[height][recoveredSignatory] = signature;
        info.currentWeight += weight;

        if (info.currentWeight >= info.threshold) {
            if (!info.reached) {
                info.reached = true;
                // quorum is completed since the threshold has been reached
                ok = self.incompleteQuorums.remove(height);
                if (!ok) {
                    revert FailedRemoveIncompleteQuorum();
                }
                emit QuorumReached({
                    objKind: self.quorumObjKind,
                    height: height,
                    objHash: info.hash,
                    quorumWeight: info.currentWeight
                });
            } else {
                emit QuorumWeightUpdated({
                    objKind: self.quorumObjKind,
                    height: height,
                    objHash: info.hash,
                    newWeight: info.currentWeight
                });
            }
        }
    }

    /// @notice creates the quorum info from a quorum object.
    /// @param  objHeight - height of the quorum object
    /// @param  objHash - hash of the object
    /// @param membershipRootHash - a root hash of the Merkle tree built from the validator public keys and their weight
    /// @param membershipWeight - the total weight of the membership
    /// @param majorityPercentage - the majorityPercentage required to reach quorum
    function createQuorumInfo(
        QuorumMap storage self,
        uint256 objHeight,
        bytes32 objHash,
        bytes32 membershipRootHash,
        uint256 membershipWeight,
        uint256 majorityPercentage
    ) internal {
        if (objHeight < self.retentionHeight) {
            revert QuorumAlreadyProcessed();
        }

        if (membershipWeight == 0) {
            revert ZeroMembershipWeight();
        }

        uint256 threshold = weightNeeded(membershipWeight, majorityPercentage);

        // process the checkpoint
        bool ok = self.incompleteQuorums.add(objHeight);
        if (!ok) {
            revert FailedAddIncompleteQuorum();
        }

        QuorumInfo memory info = QuorumInfo({
            hash: objHash,
            rootHash: membershipRootHash,
            threshold: threshold,
            currentWeight: 0,
            reached: false
        });

        // persist quorum info
        self.quorumInfo[objHeight] = info;
    }

    /// @notice Sets a new  retention height and garbage collects all checkpoints in range [`retentionHeight`, `newRetentionHeight`)
    /// @dev `retentionHeight` is the height of the first incomplete checkpointswe must keep to implement checkpointing.
    /// All checkpoints with a height less than `retentionHeight` are removed from the history, assuming they are committed to the parent.
    /// @param newRetentionHeight - the height of the oldest checkpoint to keep
    function pruneQuorums(QuorumMap storage self, uint256 newRetentionHeight) internal {
        uint256 oldRetentionHeight = self.retentionHeight;

        if (newRetentionHeight <= oldRetentionHeight) {
            revert InvalidRetentionHeight();
        }

        for (uint256 h = oldRetentionHeight; h < newRetentionHeight; ) {
            address[] memory oldValidators = self.quorumSignatureSenders[h].values();
            uint256 n = oldValidators.length;

            for (uint256 i; i < n; ) {
                delete self.quorumSignatures[h][oldValidators[i]];
                self.quorumSignatureSenders[h].remove(oldValidators[i]);
                unchecked {
                    ++i;
                }
            }

            delete self.quorumInfo[h];
            delete self.quorumSignatureSenders[h];

            unchecked {
                ++h;
            }
        }

        self.retentionHeight = newRetentionHeight;
    }

    function isHeightAlreadyProcessed(
        QuorumMap storage self,
        uint256 height
    ) internal view {
        if (height < self.retentionHeight) {
            revert QuorumAlreadyProcessed();
        }
    }

    /// @notice returns the needed weight value corresponding to the majority percentage
    /// @dev `majorityPercentage` must be a valid number
    function weightNeeded(uint256 weight, uint256 majorityPercentage) internal pure returns (uint256) {
        return (weight * majorityPercentage) / 100;
    }


    /// @notice get quorum signature bundle consisting of the info, signatories and the corresponding signatures.
    function getSignatureBundle(
        QuorumMap storage self,
        uint256 h
    )
        external
        view
        returns (
            QuorumInfo memory info,
            address[] memory signatories,
            bytes[] memory signatures
        )
    {
        info = self.quorumInfo[h];
        signatories = self.quorumSignatureSenders[h].values();
        uint256 n = signatories.length;

        signatures = new bytes[](n);

        for (uint256 i; i < n; ) {
            signatures[i] = self.quorumSignatures[h][signatories[i]];
            unchecked {
                ++i;
            }
        }

        return (info, signatories, signatures);
    }
}
