// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {QuorumObjKind} from "../structs/Quorum.sol";
import {Pausable} from "../lib/LibPausable.sol";
import {ReentrancyGuard} from "../lib/LibReentrancyGuard.sol";
import {SubnetActorModifiers} from "../lib/LibSubnetActorStorage.sol";
import {LibStaking} from "../lib/LibStaking.sol";
import {LibSubnetActor} from "../lib/LibSubnetActor.sol";

contract SubnetActorRewardFacet is SubnetActorModifiers, ReentrancyGuard, Pausable {
    /// @notice Validator claims their released collateral.
    function claim() external nonReentrant whenNotPaused {
        LibStaking.claimCollateral(msg.sender);
    }
}
