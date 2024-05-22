// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {StakingChangeLog, StakingChange, StakingOperation} from "../structs/Subnet.sol";

/// The util library for `StakingChangeLog`
library LibStakingChangeLog {
    event NewStakingChangeRequest(StakingOperation op, address validator, bytes payload, uint64 configurationNumber);

    /// @notice Validator request to update its metadata
    function metadataRequest(StakingChangeLog storage changes, address validator, bytes calldata metadata) internal {
        uint64 configurationNumber = recordChange({
            changes: changes,
            validator: validator,
            op: StakingOperation.SetMetadata,
            payload: metadata
        });

        emit NewStakingChangeRequest({
            op: StakingOperation.SetMetadata,
            validator: validator,
            payload: metadata,
            configurationNumber: configurationNumber
        });
    }

    /// @notice Records a request to update the federated power of a validator
    function federatedPowerRequest(
        StakingChangeLog storage changes,
        address validator,
        bytes calldata metadata,
        uint256 power
    ) internal {
        bytes memory payload = abi.encode(metadata, power);

        uint64 configurationNumber = recordChange({
            changes: changes,
            validator: validator,
            op: StakingOperation.SetFederatedPower,
            payload: payload
        });

        emit NewStakingChangeRequest({
            op: StakingOperation.SetFederatedPower,
            validator: validator,
            payload: payload,
            configurationNumber: configurationNumber
        });
    }

    /// @notice Perform upsert operation to the withdraw changes, return total value to withdraw
    /// @notice of the validator.
    /// Each insert will increment the configuration number by 1, update will not.
    function withdrawRequest(StakingChangeLog storage changes, address validator, uint256 amount) internal {
        bytes memory payload = abi.encode(amount);

        uint64 configurationNumber = recordChange({
            changes: changes,
            validator: validator,
            op: StakingOperation.Withdraw,
            payload: payload
        });

        emit NewStakingChangeRequest({
            op: StakingOperation.Withdraw,
            validator: validator,
            payload: payload,
            configurationNumber: configurationNumber
        });
    }

    /// @notice Perform upsert operation to the deposit changes
    function depositRequest(StakingChangeLog storage changes, address validator, uint256 amount) internal {
        bytes memory payload = abi.encode(amount);

        uint64 configurationNumber = recordChange({
            changes: changes,
            validator: validator,
            op: StakingOperation.Deposit,
            payload: payload
        });

        emit NewStakingChangeRequest({
            op: StakingOperation.Deposit,
            validator: validator,
            payload: payload,
            configurationNumber: configurationNumber
        });
    }

    /// @notice Perform upsert operation to the deposit changes
    function recordChange(
        StakingChangeLog storage changes,
        address validator,
        StakingOperation op,
        bytes memory payload
    ) internal returns (uint64 configurationNumber) {
        configurationNumber = changes.nextConfigurationNumber;

        changes.changes[configurationNumber] = StakingChange({op: op, validator: validator, payload: payload});

        changes.nextConfigurationNumber = configurationNumber + 1;
    }

    /// @notice Get the change at configuration number
    function getChange(
        StakingChangeLog storage changes,
        uint64 configurationNumber
    ) internal view returns (StakingChange storage) {
        return changes.changes[configurationNumber];
    }

    function purgeChange(StakingChangeLog storage changes, uint64 configurationNumber) internal {
        delete changes.changes[configurationNumber];
    }
}
