// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {GatewayDiamond} from "../../src/GatewayDiamond.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {SubnetActorDiamond} from "../../src/SubnetActorDiamond.sol";
import {SubnetRegistryDiamond} from "../../src/SubnetRegistryDiamond.sol";

library DiamondFacetsHelper {
    function diamondLouper(address a) internal pure returns (DiamondLoupeFacet) {
        DiamondLoupeFacet facet = DiamondLoupeFacet(a);
        return facet;
    }

    function diamondCutter(address a) internal pure returns (DiamondCutFacet) {
        DiamondCutFacet facet = DiamondCutFacet(a);
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

    //

    function diamondLouper(SubnetActorDiamond a) internal pure returns (DiamondLoupeFacet) {
        DiamondLoupeFacet facet = DiamondLoupeFacet(address(a));
        return facet;
    }

    function diamondCutter(SubnetActorDiamond a) internal pure returns (DiamondCutFacet) {
        DiamondCutFacet facet = DiamondCutFacet(address(a));
        return facet;
    }

    //

    function diamondLouper(SubnetRegistryDiamond a) internal pure returns (DiamondLoupeFacet) {
        DiamondLoupeFacet facet = DiamondLoupeFacet(address(a));
        return facet;
    }

    function diamondCutter(SubnetRegistryDiamond a) internal pure returns (DiamondCutFacet) {
        DiamondCutFacet facet = DiamondCutFacet(address(a));
        return facet;
    }
}
