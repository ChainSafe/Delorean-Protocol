// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {StdAssertions} from "forge-std/StdAssertions.sol";
import {GatewayDiamond} from "../../src/GatewayDiamond.sol";
import {IntegrationTestBase, TestGatewayActor} from "../IntegrationTestBase.sol";
import {GatewayFacetsHelper} from "../helpers/GatewayFacetsHelper.sol";

/// @title GatewayActor properties.
/// @dev It is suggested that all properties are defined here.
///     To check that a concrete GatewayActor instance holds the properties that target contract should inherit from this contract.
///     This contract must be abstract.
abstract contract GatewayActorBasicProperties is StdAssertions, TestGatewayActor {
    using GatewayFacetsHelper for GatewayDiamond;

    /// @notice The number of subnets is consistent within GatewayActor mechanisms.
    function invariant_GA_01_consistent_subnet_number() public virtual {
        assertEq(
            gatewayDiamond.getter().totalSubnets(),
            gatewayDiamond.getter().listSubnets().length,
            "the number of subnets is not consistent"
        );
    }
}
