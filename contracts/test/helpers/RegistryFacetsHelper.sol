// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {SubnetRegistryDiamond} from "../../src/SubnetRegistryDiamond.sol";
import {RegisterSubnetFacet} from "../../src/subnetregistry/RegisterSubnetFacet.sol";
import {SubnetGetterFacet} from "../../src/subnetregistry/SubnetGetterFacet.sol";

library RegistryFacetsHelper {
    function register(address a) internal pure returns (RegisterSubnetFacet) {
        RegisterSubnetFacet facet = RegisterSubnetFacet(a);
        return facet;
    }

    function getter(address a) internal pure returns (SubnetGetterFacet) {
        SubnetGetterFacet facet = SubnetGetterFacet(a);
        return facet;
    }

    //

    function register(SubnetRegistryDiamond a) internal pure returns (RegisterSubnetFacet) {
        RegisterSubnetFacet facet = RegisterSubnetFacet(address(a));
        return facet;
    }

    function getter(SubnetRegistryDiamond a) internal pure returns (SubnetGetterFacet) {
        SubnetGetterFacet facet = SubnetGetterFacet(address(a));
        return facet;
    }
}
