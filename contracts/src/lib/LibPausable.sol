// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

/// @title Pausable Library
/// @notice Abstract contract that enables contract to pause marked operations
abstract contract Pausable {
    bytes32 private constant NAMESPACE = keccak256("pausable.lib.diamond.storage");

    struct PausableStorage {
        bool paused;
    }

    /**
     * @dev Emitted when the pause is triggered by `account`.
     */
    event Paused(address account);

    /**
     * @dev Emitted when the unpause is triggered by `account`.
     */
    event Unpaused(address account);

    /**
     * @dev The operation failed because the contract is paused.
     */
    error EnforcedPause();

    /**
     * @dev The operation failed because the contract is not paused.
     */
    error ExpectedPause();

    /**
     * @dev Modifier to make a function callable only when the contract is not paused.
     *
     * Requirements:
     *
     * - The contract must not be paused.
     */
    modifier whenNotPaused() {
        _requireNotPaused();
        _;
    }

    /**
     * @dev Modifier to make a function callable only when the contract is paused.
     *
     * Requirements:
     *
     * - The contract must be paused.
     */
    modifier whenPaused() {
        _requirePaused();
        _;
    }

    /**
     * @dev Throws if the contract is paused.
     */
    function _requireNotPaused() internal view {
        if (_paused()) {
            revert EnforcedPause();
        }
    }

    /**
     * @dev Throws if the contract is not paused.
     */
    function _requirePaused() internal view {
        if (!_paused()) {
            revert ExpectedPause();
        }
    }

    /// @notice returns true if the contract is paused
    function _paused() internal view returns(bool) {
        PausableStorage storage s = pausableStorage();
        return s.paused;
    }

    /**
     * @dev Triggers stopped state.
     *
     * Requirements:
     *
     * - The contract must not be paused.
     */
    function _pause() internal {
        _requireNotPaused();
        PausableStorage storage s = pausableStorage();
        s.paused = true;
        emit Paused(msg.sender);
    }

    /**
     * @dev Returns to normal state.
     *
     * Requirements:
     *
     * - The contract must be paused.
     */
    function _unpause() internal  {
        _requirePaused();
        PausableStorage storage s = pausableStorage();
        s.paused = false;
        emit Unpaused(msg.sender);
    }

    /// @notice get the storage slot
    function pausableStorage() private pure returns (PausableStorage storage ds) {
        bytes32 position = NAMESPACE;
        // solhint-disable-next-line no-inline-assembly
        assembly {
            ds.slot := position
        }
        return ds;
    }
}
