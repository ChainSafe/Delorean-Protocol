// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {IGateway} from "../interfaces/IGateway.sol";
import {LibSubnetActorStorage, SubnetActorStorage} from "./LibSubnetActorStorage.sol";
import {LibMaxPQ, MaxPQ} from "./priority/LibMaxPQ.sol";
import {LibMinPQ, MinPQ} from "./priority/LibMinPQ.sol";
import {LibStakingChangeLog} from "./LibStakingChangeLog.sol";
import {PermissionMode, StakingReleaseQueue, StakingChangeLog, StakingChange, StakingChangeRequest, StakingOperation, StakingRelease, ValidatorSet, AddressStakingReleases, ParentValidatorsTracker, Validator} from "../structs/Subnet.sol";
import {WithdrawExceedingCollateral, NotValidator, CannotConfirmFutureChanges, NoCollateralToWithdraw, AddressShouldBeValidator, InvalidConfigurationNumber} from "../errors/IPCErrors.sol";
import {Address} from "openzeppelin-contracts/utils/Address.sol";

library LibAddressStakingReleases {
    /// @notice Add new release to the storage. Caller makes sure the release.releasedAt is ordered
    /// @notice in ascending order. This method does not do checks on this.
    function push(AddressStakingReleases storage self, StakingRelease memory release) internal {
        uint16 length = self.length;
        uint16 nextIdx = self.startIdx + length;

        self.releases[nextIdx] = release;
        self.length = length + 1;
    }

    /// @notice Perform compaction on releases, i.e. aggregates the amount that can be released
    /// @notice and removes them from storage. Returns the total amount to release and the new
    /// @notice number of pending releases after compaction.
    function compact(AddressStakingReleases storage self) internal returns (uint256, uint16) {
        uint16 length = self.length;
        if (self.length == 0) {
            revert NoCollateralToWithdraw();
        }

        uint16 i = self.startIdx;
        uint16 newLength = length;
        uint256 amount;
        while (i < length) {
            StakingRelease memory release = self.releases[i];

            // releases are ordered ascending by releaseAt, no need to check
            // further as they will still be locked.
            if (release.releaseAt > block.number) {
                break;
            }

            amount += release.amount;
            delete self.releases[i];

            unchecked {
                ++i;
                --newLength;
            }
        }

        self.startIdx = i;
        self.length = newLength;

        return (amount, newLength);
    }
}

/// The util library for `StakingReleaseQueue`
library LibStakingReleaseQueue {
    using Address for address payable;
    using LibAddressStakingReleases for AddressStakingReleases;

    event NewCollateralRelease(address validator, uint256 amount, uint256 releaseBlock);

    function setLockDuration(StakingReleaseQueue storage self, uint256 blocks) internal {
        self.lockingDuration = blocks;
    }

    /// @notice Set the amount and time for release collateral
    function addNewRelease(StakingReleaseQueue storage self, address validator, uint256 amount) internal {
        uint256 releaseAt = block.number + self.lockingDuration;
        StakingRelease memory release = StakingRelease({releaseAt: releaseAt, amount: amount});

        self.releases[validator].push(release);

        emit NewCollateralRelease({validator: validator, amount: amount, releaseBlock: releaseAt});
    }

    /// @notice Validator claim the available collateral that are released
    function claim(StakingReleaseQueue storage self, address validator) internal returns (uint256) {
        (uint256 amount, uint16 newLength) = self.releases[validator].compact();

        if (newLength == 0) {
            delete self.releases[validator];
        }

        payable(validator).sendValue(amount);

        return amount;
    }
}

/// The util library for `ValidatorSet`
library LibValidatorSet {
    using LibMinPQ for MinPQ;
    using LibMaxPQ for MaxPQ;

    event ActiveValidatorCollateralUpdated(address validator, uint256 newPower);
    event WaitingValidatorCollateralUpdated(address validator, uint256 newPower);
    event NewActiveValidator(address validator, uint256 power);
    event NewWaitingValidator(address validator, uint256 power);
    event ActiveValidatorReplaced(address oldValidator, address newValidator);
    event ActiveValidatorLeft(address validator);
    event WaitingValidatorLeft(address validator);

    /// @notice Get the total voting power for the validator
    function getPower(
        ValidatorSet storage validators,
        address validator
    ) internal view returns(uint256 power) {
        if (validators.permissionMode == PermissionMode.Federated) {
            power = validators.validators[validator].federatedPower;
        } else {
            power = validators.validators[validator].confirmedCollateral;
        }
    }

    /// @notice Get the total confirmed collateral of the validators.
    function getTotalConfirmedCollateral(ValidatorSet storage validators) internal view returns (uint256 collateral) {
        collateral = validators.totalConfirmedCollateral;
    }

    /// @notice Get the total active validators.
    function totalActiveValidators(ValidatorSet storage validators) internal view returns (uint16 total) {
        total = validators.activeValidators.getSize();
    }

    /// @notice Get the confirmed collateral of the validator.
    function getConfirmedCollateral(
        ValidatorSet storage validators,
        address validator
    ) internal view returns (uint256 collateral) {
        collateral = validators.validators[validator].confirmedCollateral;
    }

    function listActiveValidators(ValidatorSet storage validators) internal view returns (address[] memory addresses) {
        uint16 size = validators.activeValidators.getSize();
        addresses = new address[](size);
        for (uint16 i = 1; i <= size; ) {
            addresses[i - 1] = validators.activeValidators.getAddress(i);
            unchecked {
                ++i;
            }
        }
        return addresses;
    }

    /// @notice Get the total collateral of *active* validators.
    function getTotalActivePower(ValidatorSet storage validators) internal view returns (uint256 collateral) {
        uint16 size = validators.activeValidators.getSize();
        for (uint16 i = 1; i <= size; ) {
            address validator = validators.activeValidators.getAddress(i);
            collateral += getPower(validators, validator);
            unchecked {
                ++i;
            }
        }
    }

    /// @notice Get the total collateral of the *waiting* and *active* validators.
    function getTotalCollateral(ValidatorSet storage validators) internal view returns (uint256 collateral) {
        uint16 size = validators.waitingValidators.getSize();
        for (uint16 i = 1; i <= size; ) {
            address validator = validators.waitingValidators.getAddress(i);
            collateral += getConfirmedCollateral(validators, validator);
            unchecked {
                ++i;
            }
        }
        collateral += getTotalConfirmedCollateral(validators);
    }


    /// @notice Get the total power of the validators.
    /// The function reverts if at least one validator is not in the active validator set.
    function getTotalPowerOfValidators(
        ValidatorSet storage validators,
        address[] memory addresses
    ) internal view returns (uint256[] memory) {
        uint256 size = addresses.length;
        uint256[] memory activePowerTable = new uint256[](size);

        for (uint256 i; i < size; ) {
            if (!isActiveValidator(validators, addresses[i])) {
                revert NotValidator(addresses[i]);
            }
            activePowerTable[i] = getPower(validators, addresses[i]);
            unchecked {
                ++i;
            }
        }
        return activePowerTable;
    }

    function isActiveValidator(ValidatorSet storage self, address validator) internal view returns (bool) {
        return self.activeValidators.contains(validator);
    }

    /// @notice Set validator data
    function setMetadata(ValidatorSet storage validators, address validator, bytes calldata metadata) internal {
        validators.validators[validator].metadata = metadata;
    }

    /***********************************************************************
     * Internal helper functions, should not be called by external functions
     ***********************************************************************/

    /// @notice Validator increases its total collateral by amount.
    function recordDeposit(ValidatorSet storage validators, address validator, uint256 amount) internal {
        validators.validators[validator].totalCollateral += amount;
    }

    /// @notice Validator reduces its total collateral by amount.
    function recordWithdraw(ValidatorSet storage validators, address validator, uint256 amount) internal {
        uint256 total = validators.validators[validator].totalCollateral;
        if (total < amount) {
            revert WithdrawExceedingCollateral();
        }

        total -= amount;
        validators.validators[validator].totalCollateral = total;
    }

    /// @notice Validator's federated power was updated by admin
    function confirmFederatedPower(ValidatorSet storage self, address validator, uint256 power) internal {
        uint256 existingPower = self.validators[validator].federatedPower;
        self.validators[validator].federatedPower = power;

        if (existingPower == power) {
            return;
        } else if (existingPower < power) {
            increaseReshuffle({self: self, maybeActive: validator, newPower: power});
        } else {
            reduceReshuffle({self: self, validator: validator, newPower: power});
        }
    }

    function confirmDeposit(ValidatorSet storage self, address validator, uint256 amount) internal {
        uint256 newCollateral = self.validators[validator].confirmedCollateral + amount;
        self.validators[validator].confirmedCollateral = newCollateral;

        self.totalConfirmedCollateral += amount;

        increaseReshuffle({self: self, maybeActive: validator, newPower: newCollateral});
    }

    function confirmWithdraw(ValidatorSet storage self, address validator, uint256 amount) internal {
        uint256 newCollateral = self.validators[validator].confirmedCollateral - amount;
        uint256 totalCollateral = self.validators[validator].totalCollateral;

        if (newCollateral == 0 && totalCollateral == 0) {
            delete self.validators[validator];
        } else {
            self.validators[validator].confirmedCollateral = newCollateral;
        }

        reduceReshuffle({self: self, validator: validator, newPower: newCollateral});

        self.totalConfirmedCollateral -= amount;
    }

    /// @notice Reshuffles the active and waiting validators when an increase in power is confirmed
    function increaseReshuffle(ValidatorSet storage self, address maybeActive, uint256 newPower) internal {
        if (self.activeValidators.contains(maybeActive)) {
            self.activeValidators.increaseReheapify(self, maybeActive);
            emit ActiveValidatorCollateralUpdated(maybeActive, newPower);
            return;
        }

        // incoming address is not active validator
        uint16 activeLimit = self.activeLimit;
        uint16 activeSize = self.activeValidators.getSize();
        if (activeLimit > activeSize) {
            // we can still take more active validators, just insert to the pq.
            self.activeValidators.insert(self, maybeActive);
            emit NewActiveValidator(maybeActive, newPower);
            return;
        }

        // now we have enough active validators, we need to check:
        // - if the incoming new collateral is more than the min active collateral,
        //     - yes:
        //        - pop the min active validator
        //        - remove the incoming validator from waiting validators
        //        - insert incoming validator into active validators
        //        - insert popped validator into waiting validators
        //     - no:
        //        - insert the incoming validator into waiting validators
        (address minAddress, uint256 minActivePower) = self.activeValidators.min(self);
        if (minActivePower < newPower) {
            self.activeValidators.pop(self);

            if (self.waitingValidators.contains(maybeActive)) {
                self.waitingValidators.deleteReheapify(self, maybeActive);
            }

            self.activeValidators.insert(self, maybeActive);
            self.waitingValidators.insert(self, minAddress);

            emit ActiveValidatorReplaced(minAddress, maybeActive);
            return;
        }

        if (self.waitingValidators.contains(maybeActive)) {
            self.waitingValidators.increaseReheapify(self, maybeActive);
            emit WaitingValidatorCollateralUpdated(maybeActive, newPower);
            return;
        }

        self.waitingValidators.insert(self, maybeActive);
        emit NewWaitingValidator(maybeActive, newPower);
    }

    /// @notice Reshuffles the active and waiting validators when a power reduction is confirmed
    function reduceReshuffle(ValidatorSet storage self, address validator, uint256 newPower) internal {
        if (self.waitingValidators.contains(validator)) {
            if (newPower == 0) {
                self.waitingValidators.deleteReheapify(self, validator);
                emit WaitingValidatorLeft(validator);
                return;
            }
            self.waitingValidators.decreaseReheapify(self, validator);
            emit WaitingValidatorCollateralUpdated(validator, newPower);
            return;
        }

        // sanity check
        if (!self.activeValidators.contains(validator)) {
            revert AddressShouldBeValidator();
        }

        // the validator is an active validator!

        if (newPower == 0) {
            self.activeValidators.deleteReheapify(self, validator);
            emit ActiveValidatorLeft(validator);

            if (self.waitingValidators.getSize() != 0) {
                (address toBePromoted, uint256 power) = self.waitingValidators.max(self);
                self.waitingValidators.pop(self);
                self.activeValidators.insert(self, toBePromoted);
                emit NewActiveValidator(toBePromoted, power);
            }

            return;
        }

        self.activeValidators.decreaseReheapify(self, validator);

        if (self.waitingValidators.getSize() == 0) {
            return;
        }

        (address mayBeDemoted, uint256 minActivePower) = self.activeValidators.min(self);
        (address mayBePromoted, uint256 maxWaitingPower) = self.waitingValidators.max(self);
        if (minActivePower < maxWaitingPower) {
            self.activeValidators.pop(self);
            self.waitingValidators.pop(self);
            self.activeValidators.insert(self, mayBePromoted);
            self.waitingValidators.insert(self, mayBeDemoted);

            emit ActiveValidatorReplaced(mayBeDemoted, mayBePromoted);
            return;
        }

        emit ActiveValidatorCollateralUpdated(validator, newPower);
    }
}

library LibStaking {
    using LibStakingReleaseQueue for StakingReleaseQueue;
    using LibStakingChangeLog for StakingChangeLog;
    using LibValidatorSet for ValidatorSet;
    using LibMaxPQ for MaxPQ;
    using LibMinPQ for MinPQ;
    using Address for address payable;

    uint64 internal constant INITIAL_CONFIGURATION_NUMBER = 1;

    event ConfigurationNumberConfirmed(uint64 number);
    event CollateralClaimed(address validator, uint256 amount);

    // =============== Getters =============
    function getPower(
        address validator
    ) internal view returns(uint256 power) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.getPower(validator);
    }

    /// @notice Checks if the validator is an active validator
    function isActiveValidator(address validator) internal view returns (bool) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.activeValidators.contains(validator);
    }

    /// @notice Checks if the validator is a waiting validator
    function isWaitingValidator(address validator) internal view returns (bool) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.waitingValidators.contains(validator);
    }

    /// @notice Checks if the provided address is a validator (active or waiting) based on its total collateral.
    /// @param addr The address to check for validator status.
    /// @return A boolean indicating whether the address is a validator.
    function isValidator(address addr) internal view returns (bool) {
        return hasStaked(addr);
    }

    /// @notice Checks if the validator has staked before.
    /// @param validator The address to check for staking status.
    /// @return A boolean indicating whether the validator has staked.
    function hasStaked(address validator) internal view returns (bool) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();

        // gas-opt: original check: totalCollateral > 0
        return s.validatorSet.validators[validator].totalCollateral != 0;
    }

    function totalActiveValidators() internal view returns (uint16) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.totalActiveValidators();
    }

    /// @notice Gets the total number of validators, including active and waiting
    function totalValidators() internal view returns (uint16) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.waitingValidators.getSize() + s.validatorSet.activeValidators.getSize();
    }

    function getTotalConfirmedCollateral() internal view returns (uint256) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.getTotalConfirmedCollateral();
    }

    function getTotalCollateral() internal view returns (uint256) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.getTotalConfirmedCollateral();
    }

    /// @notice Gets the total collateral the validators has staked.
    function totalValidatorCollateral(address validator) internal view returns (uint256) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return s.validatorSet.validators[validator].totalCollateral;
    }

    // =============== Operations directly confirm =============

    /// @notice Set the validator federated power directly without queueing the request
    function setFederatedPowerWithConfirm(address validator, uint256 power) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        s.validatorSet.confirmFederatedPower(validator, power);
    }

    /// @notice Set the validator metadata directly without queueing the request
    function setMetadataWithConfirm(address validator, bytes calldata metadata) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        s.validatorSet.setMetadata(validator, metadata);
    }

    /// @notice Confirm the deposit directly without going through the confirmation process
    function depositWithConfirm(address validator, uint256 amount) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();

        // record deposit that updates the total collateral
        s.validatorSet.recordDeposit(validator, amount);
        // confirm deposit that updates the confirmed collateral
        s.validatorSet.confirmDeposit(validator, amount);

        if (!s.bootstrapped) {
            // add to initial validators avoiding duplicates if it
            // is a genesis validator.
            bool alreadyValidator;
            uint256 length = s.genesisValidators.length;
            for (uint256 i; i < length; ) {
                if (s.genesisValidators[i].addr == validator) {
                    alreadyValidator = true;
                    break;
                }
                unchecked {
                    ++i;
                }
            }
            if (!alreadyValidator) {
                uint256 collateral = s.validatorSet.validators[validator].confirmedCollateral;
                Validator memory val = Validator({
                    addr: validator,
                    weight: collateral,
                    metadata: s.validatorSet.validators[validator].metadata
                });
                s.genesisValidators.push(val);
            }
        }
    }

    /// @notice Confirm the withdraw directly without going through the confirmation process
    /// and releasing from the gateway.
    /// @dev only use for non-bootstrapped subnets
    function withdrawWithConfirm(address validator, uint256 amount) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();

        // record deposit that updates the total collateral
        s.validatorSet.recordWithdraw(validator, amount);
        // confirm deposit that updates the confirmed collateral
        s.validatorSet.confirmWithdraw(validator, amount);

        // release stake from gateway and transfer to user
        payable(validator).sendValue(amount);
    }

    // ================= Operations that are queued ==============
    /// @notice Set the federated power of the validator
    function setFederatedPower(address validator, bytes calldata metadata, uint256 amount) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        s.changeSet.federatedPowerRequest({validator: validator, metadata: metadata, power: amount});
    }

    /// @notice Set the validator metadata
    function setValidatorMetadata(address validator, bytes calldata metadata) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        s.changeSet.metadataRequest(validator, metadata);
    }

    /// @notice Deposit the collateral
    function deposit(address validator, uint256 amount) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();

        s.changeSet.depositRequest(validator, amount);
        s.validatorSet.recordDeposit(validator, amount);
    }

    /// @notice Withdraw the collateral
    function withdraw(address validator, uint256 amount) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();

        s.changeSet.withdrawRequest(validator, amount);
        s.validatorSet.recordWithdraw(validator, amount);
    }

    // =============== Other functions ================

    /// @notice Claim the released collateral
    function claimCollateral(address validator) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        uint256 amount = s.releaseQueue.claim(validator);
        emit CollateralClaimed(validator, amount);
    }

    function getConfigurationNumbers() internal view returns(uint64, uint64) {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        return (s.changeSet.nextConfigurationNumber, s.changeSet.startConfigurationNumber);
    }

    /// @notice Confirm the changes in bottom up checkpoint submission, only call this in bottom up checkpoint execution.
    function confirmChange(uint64 configurationNumber) internal {
        SubnetActorStorage storage s = LibSubnetActorStorage.appStorage();
        StakingChangeLog storage changeSet = s.changeSet;

        if (configurationNumber >= changeSet.nextConfigurationNumber) {
            revert CannotConfirmFutureChanges();
        } else if (configurationNumber < changeSet.startConfigurationNumber) {
            return;
        }

        uint64 start = changeSet.startConfigurationNumber;
        for (uint64 i = start; i <= configurationNumber; ) {
            StakingChange storage change = changeSet.getChange(i);
            address validator = change.validator;

            if (change.op == StakingOperation.SetMetadata) {
                s.validatorSet.validators[validator].metadata = change.payload;
            } else if (change.op == StakingOperation.SetFederatedPower) {
                (bytes memory metadata, uint256 power) = abi.decode(change.payload, (bytes, uint256));
                s.validatorSet.validators[validator].metadata = metadata;
                s.validatorSet.confirmFederatedPower(validator, power);
            } else {
                uint256 amount = abi.decode(change.payload, (uint256));

                if (change.op == StakingOperation.Withdraw) {
                    s.validatorSet.confirmWithdraw(validator, amount);
                    s.releaseQueue.addNewRelease(validator, amount);
                    IGateway(s.ipcGatewayAddr).releaseStake(amount);
                } else {
                    s.validatorSet.confirmDeposit(validator, amount);
                    IGateway(s.ipcGatewayAddr).addStake{value: amount}();
                }
            }

            changeSet.purgeChange(i);
            unchecked {
                ++i;
            }
        }

        changeSet.startConfigurationNumber = configurationNumber + 1;

        emit ConfigurationNumberConfirmed(configurationNumber);
    }
}

/// The library for tracking validator changes coming from the parent.
/// Should be used in the child gateway to store changes until they can be applied.
library LibValidatorTracking {
    using LibValidatorSet for ValidatorSet;
    using LibStakingChangeLog for StakingChangeLog;

    function storeChange(ParentValidatorsTracker storage self, StakingChangeRequest calldata changeRequest) internal {
        uint64 configurationNumber = self.changes.recordChange({
            validator: changeRequest.change.validator,
            op: changeRequest.change.op,
            payload: changeRequest.change.payload
        });

        if (configurationNumber != changeRequest.configurationNumber) {
            revert InvalidConfigurationNumber();
        }
    }

    function batchStoreChange(
        ParentValidatorsTracker storage self,
        StakingChangeRequest[] calldata changeRequests
    ) internal {
        uint256 length = changeRequests.length;
        if (length == 0) {
            return;
        }

        for (uint256 i; i < length; ) {
            storeChange(self, changeRequests[i]);
            unchecked {
                ++i;
            }
        }
    }

    /// @notice Confirm the changes in for a finality commitment
    function confirmChange(ParentValidatorsTracker storage self, uint64 configurationNumber) internal {
        if (configurationNumber >= self.changes.nextConfigurationNumber) {
            revert CannotConfirmFutureChanges();
        } else if (configurationNumber < self.changes.startConfigurationNumber) {
            return;
        }

        uint64 start = self.changes.startConfigurationNumber;

        for (uint64 i = start; i <= configurationNumber; ) {
            StakingChange storage change = self.changes.getChange(i);
            address validator = change.validator;

            if (change.op == StakingOperation.SetMetadata) {
                self.validators.validators[validator].metadata = change.payload;
            } else if (change.op == StakingOperation.SetFederatedPower) {
                (bytes memory metadata, uint256 power) = abi.decode(change.payload, (bytes, uint256));
                self.validators.validators[validator].metadata = metadata;
                self.validators.confirmFederatedPower(validator, power);
            } else {
                uint256 amount = abi.decode(change.payload, (uint256));

                if (change.op == StakingOperation.Withdraw) {
                    self.validators.confirmWithdraw(validator, amount);
                } else {
                    self.validators.confirmDeposit(validator, amount);
                }
            }

            self.changes.purgeChange(i);
            unchecked {
                ++i;
            }
        }
        self.changes.startConfigurationNumber = configurationNumber + 1;
    }
}
