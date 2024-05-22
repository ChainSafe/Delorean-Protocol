// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {InvalidBatchEpoch, MaxMsgsPerBatchExceeded, InvalidSignatureErr, BottomUpCheckpointAlreadySubmitted, CannotSubmitFutureCheckpoint, InvalidCheckpointEpoch} from "../errors/IPCErrors.sol";
import {IGateway} from "../interfaces/IGateway.sol";
import {BottomUpCheckpoint, BottomUpMsgBatch, BottomUpMsgBatchInfo} from "../structs/CrossNet.sol";
import {Validator, ValidatorSet} from "../structs/Subnet.sol";
import {MultisignatureChecker} from "../lib/LibMultisignatureChecker.sol";
import {ReentrancyGuard} from "../lib/LibReentrancyGuard.sol";
import {SubnetActorModifiers} from "../lib/LibSubnetActorStorage.sol";
import {LibValidatorSet, LibStaking} from "../lib/LibStaking.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";
import {LibSubnetActor} from "../lib/LibSubnetActor.sol";
import {Pausable} from "../lib/LibPausable.sol";
import {LibGateway} from "../lib/LibGateway.sol";

contract SubnetActorCheckpointingFacet is SubnetActorModifiers, ReentrancyGuard, Pausable {
    using EnumerableSet for EnumerableSet.AddressSet;
    using LibValidatorSet for ValidatorSet;

    /// @notice Submits a checkpoint commitment for execution.
    /// @dev    It triggers the commitment of the checkpoint and any other side-effects that
    ///         need to be triggered by the checkpoint such as relayer reward book keeping.
    /// @param checkpoint The executed bottom-up checkpoint.
    /// @param signatories The addresses of validators signing the checkpoint.
    /// @param signatures The signatures of validators on the checkpoint.
    function submitCheckpoint(
        BottomUpCheckpoint calldata checkpoint,
        address[] calldata signatories,
        bytes[] calldata signatures
    ) external whenNotPaused {
        ensureValidCheckpoint(checkpoint);

        bytes32 checkpointHash = keccak256(abi.encode(checkpoint));

        // validate signatures and quorum threshold, revert if validation fails
        validateActiveQuorumSignatures({signatories: signatories, hash: checkpointHash, signatures: signatures});

        // If the checkpoint height is the next expected height then this is a new checkpoint which must be executed
        // in the Gateway Actor, the checkpoint and the relayer must be stored, last bottom-up checkpoint updated.
        s.committedCheckpoints[checkpoint.blockHeight] = checkpoint;

        s.lastBottomUpCheckpointHeight = checkpoint.blockHeight;

        // Commit in gateway to distribute rewards
        IGateway(s.ipcGatewayAddr).commitCheckpoint(checkpoint);

        // confirming the changes in membership in the child
        LibStaking.confirmChange(checkpoint.nextConfigurationNumber);
    }

    /// @notice Checks whether the signatures are valid for the provided signatories and hash within the current validator set.
    ///         Reverts otherwise.
    /// @dev Signatories in `signatories` and their signatures in `signatures` must be provided in the same order.
    ///       Having it public allows external users to perform sanity-check verification if needed.
    /// @param signatories The addresses of the signatories.
    /// @param hash The hash of the checkpoint.
    /// @param signatures The packed signatures of the checkpoint.
    function validateActiveQuorumSignatures(
        address[] memory signatories,
        bytes32 hash,
        bytes[] memory signatures
    ) public view {
        // This call reverts if at least one of the signatories (validator) is not in the active validator set.
        uint256[] memory collaterals = s.validatorSet.getTotalPowerOfValidators(signatories);
        uint256 activeCollateral = s.validatorSet.getTotalActivePower();

        uint256 threshold = (activeCollateral * s.majorityPercentage) / 100;

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature({
            signatories: signatories,
            weights: collaterals,
            threshold: threshold,
            hash: hash,
            signatures: signatures
        });

        if (!valid) {
            revert InvalidSignatureErr(uint8(err));
        }
    }

    /// @notice Ensures the checkpoint is valid.
    /// @dev The checkpoint block height must be equal to the last bottom-up checkpoint height or
    /// @dev the next one or the number of bottom up messages exceeds the max batch size.
    function ensureValidCheckpoint(BottomUpCheckpoint calldata checkpoint) internal view {
        uint64 maxMsgsPerBottomUpBatch = s.maxMsgsPerBottomUpBatch;
        if (checkpoint.msgs.length > maxMsgsPerBottomUpBatch) {
            revert MaxMsgsPerBatchExceeded();
        }

        uint256 lastBottomUpCheckpointHeight = s.lastBottomUpCheckpointHeight;
        uint256 bottomUpCheckPeriod = s.bottomUpCheckPeriod;

        // cannot submit past bottom up checkpoint
        if (checkpoint.blockHeight <= lastBottomUpCheckpointHeight) {
            revert BottomUpCheckpointAlreadySubmitted();
        }

        uint256 nextCheckpointHeight = LibGateway.getNextEpoch(lastBottomUpCheckpointHeight, bottomUpCheckPeriod);

        if (checkpoint.blockHeight > nextCheckpointHeight) {
            revert CannotSubmitFutureCheckpoint();
        }

        // the expected bottom up checkpoint height, valid height
        if (checkpoint.blockHeight == nextCheckpointHeight) {
            return;
        }

        // if the bottom up messages' length is max, we consider that epoch valid, allow early submission
        if (checkpoint.msgs.length == s.maxMsgsPerBottomUpBatch) {
            return;
        }

        revert InvalidCheckpointEpoch();
    }
}
