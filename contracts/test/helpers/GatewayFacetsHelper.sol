// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {OwnershipFacet} from "../../src/OwnershipFacet.sol";
import {GatewayGetterFacet} from "../../src/gateway/GatewayGetterFacet.sol";
import {GatewayManagerFacet} from "../../src/gateway/GatewayManagerFacet.sol";
import {GatewayMessengerFacet} from "../../src/gateway/GatewayMessengerFacet.sol";
import {TopDownFinalityFacet} from "../../src/gateway/router/TopDownFinalityFacet.sol";
import {CheckpointingFacet} from "../../src/gateway/router/CheckpointingFacet.sol";
import {XnetMessagingFacet} from "../../src/gateway/router/XnetMessagingFacet.sol";
import {GatewayDiamond} from "../../src/GatewayDiamond.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";

library GatewayFacetsHelper {
    function ownership(address gw) internal pure returns (OwnershipFacet) {
        OwnershipFacet facet = OwnershipFacet(gw);
        return facet;
    }

    function getter(address gw) internal pure returns (GatewayGetterFacet) {
        GatewayGetterFacet facet = GatewayGetterFacet(gw);
        return facet;
    }

    function manager(address gw) internal pure returns (GatewayManagerFacet) {
        GatewayManagerFacet facet = GatewayManagerFacet(gw);
        return facet;
    }

    function messenger(address gw) internal pure returns (GatewayMessengerFacet) {
        GatewayMessengerFacet facet = GatewayMessengerFacet(gw);
        return facet;
    }

    function topDownFinalizer(address gw) internal pure returns (TopDownFinalityFacet) {
        TopDownFinalityFacet facet = TopDownFinalityFacet(gw);
        return facet;
    }

    function checkpointer(address gw) internal pure returns (CheckpointingFacet) {
        CheckpointingFacet facet = CheckpointingFacet(gw);
        return facet;
    }

    function xnetMessenger(address gw) internal pure returns (XnetMessagingFacet) {
        XnetMessagingFacet facet = XnetMessagingFacet(gw);
        return facet;
    }

    //
    function ownership(GatewayDiamond gw) internal pure returns (OwnershipFacet) {
        OwnershipFacet facet = OwnershipFacet(address(gw));
        return facet;
    }

    function getter(GatewayDiamond gw) internal pure returns (GatewayGetterFacet) {
        GatewayGetterFacet facet = GatewayGetterFacet(address(gw));
        return facet;
    }

    function manager(GatewayDiamond gw) internal pure returns (GatewayManagerFacet) {
        GatewayManagerFacet facet = GatewayManagerFacet(address(gw));
        return facet;
    }

    function messenger(GatewayDiamond gw) internal pure returns (GatewayMessengerFacet) {
        GatewayMessengerFacet facet = GatewayMessengerFacet(address(gw));
        return facet;
    }

    function topDownFinalizer(GatewayDiamond gw) internal pure returns (TopDownFinalityFacet) {
        TopDownFinalityFacet facet = TopDownFinalityFacet(address(gw));
        return facet;
    }

    function checkpointer(GatewayDiamond gw) internal pure returns (CheckpointingFacet) {
        CheckpointingFacet facet = CheckpointingFacet(address(gw));
        return facet;
    }

    function xnetMessenger(GatewayDiamond gw) internal pure returns (XnetMessagingFacet) {
        XnetMessagingFacet facet = XnetMessagingFacet(address(gw));
        return facet;
    }

    //

    function diamondLouper(GatewayDiamond a) internal pure returns (DiamondLoupeFacet) {
        DiamondLoupeFacet facet = DiamondLoupeFacet(address(a));
        return facet;
    }

    function diamondCutter(GatewayDiamond a) internal pure returns (DiamondCutFacet) {
        DiamondCutFacet facet = DiamondCutFacet(address(a));
        return facet;
    }
}
