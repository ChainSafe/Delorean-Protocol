// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {GatewayActorModifiers} from "../../lib/LibGatewayActorStorage.sol";
import {ParentFinality} from "../../structs/CrossNet.sol";
import {PermissionMode, Validator, ValidatorInfo, StakingChangeRequest, Membership} from "../../structs/Subnet.sol";
import {LibGateway} from "../../lib/LibGateway.sol";

import {FilAddress} from "fevmate/utils/FilAddress.sol";

import {ParentValidatorsTracker, ValidatorSet} from "../../structs/Subnet.sol";
import {LibValidatorTracking, LibValidatorSet} from "../../lib/LibStaking.sol";

contract TopDownFinalityFacet is GatewayActorModifiers {
    using FilAddress for address;
    using LibValidatorTracking for ParentValidatorsTracker;
    using LibValidatorSet for ValidatorSet;

    /// @notice commit the ipc parent finality into storage and returns the previous committed finality
    /// This is useful to understand if the finalities are consistent or if there have been reorgs.
    /// If there are no previous committed fainality, it will be default to zero values, i.e. zero height and block hash.
    /// @param finality - the parent finality
    /// @return hasCommittedBefore A flag that indicates if a finality record has been committed before.
    /// @return previousFinality The previous finality information.
    function commitParentFinality(
        ParentFinality calldata finality
    ) external systemActorOnly returns (bool hasCommittedBefore, ParentFinality memory previousFinality) {
        previousFinality = LibGateway.commitParentFinality(finality);
        hasCommittedBefore = previousFinality.height != 0;
    }

    /// @notice Store the validator change requests from parent.
    /// @param changeRequests - the validator changes
    function storeValidatorChanges(StakingChangeRequest[] calldata changeRequests) external systemActorOnly {
        s.validatorsTracker.batchStoreChange(changeRequests);
    }

    /// @notice Returns the next and start configuration numbers in the tracker of changes
    /// from the parent in the child gateway
    function getTrackerConfigurationNumbers() external view returns (uint64, uint64) {
        return (
            s.validatorsTracker.changes.nextConfigurationNumber,
            s.validatorsTracker.changes.startConfigurationNumber
        );
    }

    /// @notice Apply all changes committed through the commitment of parent finality.
    /// @return configurationNumber The configuration number of the changes set that has been confirmed.
    function applyFinalityChanges() external systemActorOnly returns (uint64) {
        // get the latest configuration number for the change set
        uint64 configurationNumber = s.validatorsTracker.changes.nextConfigurationNumber - 1;
        // return immediately if there are no changes to confirm by looking at next configNumber
        if (
            // nextConfiguration == startConfiguration (i.e. no changes)
            (configurationNumber + 1) == s.validatorsTracker.changes.startConfigurationNumber
        ) {
            // 0 flags that there are no changes
            return 0;
        }
        // confirm the change
        s.validatorsTracker.confirmChange(configurationNumber);

        // Get active validators and populate the new power table.
        address[] memory validators = s.validatorsTracker.validators.listActiveValidators();
        uint256 vLength = validators.length;
        Validator[] memory vs = new Validator[](vLength);
        for (uint256 i; i < vLength; ) {
            address addr = validators[i];
            ValidatorInfo storage info = s.validatorsTracker.validators.validators[addr];

            // Extract the consensus weight for validator.
            uint256 weight = info.confirmedCollateral + info.federatedPower;

            vs[i] = Validator({weight: weight, addr: addr, metadata: info.metadata});
            unchecked {
                ++i;
            }
        }

        // update membership with the resulting power table.
        LibGateway.updateMembership(Membership({configurationNumber: configurationNumber, validators: vs}));
        return configurationNumber;
    }
}
