// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {SubnetActorManagerFacet} from "../../src/subnet/SubnetActorManagerFacet.sol";
import {LibStaking} from "../../src/lib/LibStaking.sol";
import {SubnetActorPauseFacet} from "../../src/subnet/SubnetActorPauseFacet.sol";
import {SubnetActorRewardFacet} from "../../src/subnet/SubnetActorRewardFacet.sol";
import {SubnetActorCheckpointingFacet} from "../../src/subnet/SubnetActorCheckpointingFacet.sol";

contract SubnetActorMock is
    SubnetActorPauseFacet,
    SubnetActorManagerFacet,
    SubnetActorRewardFacet,
    SubnetActorCheckpointingFacet
{
    function confirmChange(uint64 _configurationNumber) external {
        LibStaking.confirmChange(_configurationNumber);
    }

    function confirmNextChange() external {
        (uint64 nextConfigNum, ) = LibStaking.getConfigurationNumbers();
        LibStaking.confirmChange(nextConfigNum - 1);
    }
}
