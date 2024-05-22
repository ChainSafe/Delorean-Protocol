// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {LibDiamond} from "../lib/LibDiamond.sol";
import {Pausable} from "../lib/LibPausable.sol";

contract SubnetActorPauseFacet is Pausable {
    /// @notice Pauses all contract functions with the `whenNotPaused` modifier.
    function pause() external {
        LibDiamond.enforceIsContractOwner();
        _pause();
    }

    /// @notice Unpauses all contract functions with the `whenNotPaused` modifier.
    function unpause() external {
        LibDiamond.enforceIsContractOwner();
        _unpause();
    }

    /// @notice Returns true if the SubnetActor contract is paused.
    function paused() external view returns (bool) {
        return _paused();
    }
}
