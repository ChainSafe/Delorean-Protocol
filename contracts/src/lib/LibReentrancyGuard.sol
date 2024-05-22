// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

/// @title Reentrancy Guard
/// @notice Abstract contract to provide protection against reentrancy
abstract contract ReentrancyGuard {
    bytes32 private constant NAMESPACE = keccak256("reentrancyguard.lib.diamond.storage");

    struct ReentrancyStorage {
        uint256 status;
    }

    error ReentrancyError();

    uint256 private constant _NOT_ENTERED = 0;
    uint256 private constant _ENTERED = 1;

    modifier nonReentrant() {
        ReentrancyStorage storage s = reentrancyStorage();
        if (s.status == _ENTERED) revert ReentrancyError();
        s.status = _ENTERED;
        _;
        s.status = _NOT_ENTERED;
    }

    /// @dev fetch local storage
    function reentrancyStorage() private pure returns (ReentrancyStorage storage ds) {
        bytes32 position = NAMESPACE;
        // solhint-disable-next-line no-inline-assembly
        assembly {
            ds.slot := position
        }
        return ds;
    }
}
