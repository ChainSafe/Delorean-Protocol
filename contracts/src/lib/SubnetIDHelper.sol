// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {SubnetID} from "../structs/Subnet.sol";
import {Strings} from "openzeppelin-contracts/utils/Strings.sol";

/// @title Helper library for manipulating SubnetID struct
/// @author LimeChain team
library SubnetIDHelper {
    using Strings for address;

    error NoParentForSubnet();
    error NoAddressForRoot();
    error EmptySubnet();
    error DifferentRootNetwork();
    error InvalidRoute();

    function getAddress(SubnetID memory subnet) public pure returns (address) {
        uint256 length = subnet.route.length;

        if (length == 0) {
            revert NoAddressForRoot();
        }
        return subnet.route[length - 1];
    }

    function getParentSubnet(SubnetID memory subnet) public pure returns (SubnetID memory) {
        if (subnet.route.length == 0) {
            revert NoParentForSubnet();
        }

        address[] memory route = new address[](subnet.route.length - 1);
        uint256 routeLength = route.length;
        for (uint256 i; i < routeLength; ) {
            route[i] = subnet.route[i];
            unchecked {
                ++i;
            }
        }

        return SubnetID({root: subnet.root, route: route});
    }

    function toString(SubnetID calldata subnet) public pure returns (string memory) {
        string memory route = string.concat("/r", Strings.toString(subnet.root));
        uint256 subnetLength = subnet.route.length;
        for (uint256 i; i < subnetLength; ) {
            route = string.concat(route, "/");
            route = string.concat(route, subnet.route[i].toHexString());
            unchecked {
                ++i;
            }
        }

        return route;
    }

    function toHash(SubnetID calldata subnet) public pure returns (bytes32) {
        return keccak256(abi.encode(subnet));
    }

    function createSubnetId(SubnetID calldata subnet, address actor) public pure returns (SubnetID memory newSubnet) {
        newSubnet.root = subnet.root;
        uint256 subnetRouteLength = subnet.route.length;
        newSubnet.route = new address[](subnetRouteLength + 1);
        for (uint256 i; i < subnetRouteLength; ) {
            newSubnet.route[i] = subnet.route[i];
            unchecked {
                ++i;
            }
        }

        newSubnet.route[newSubnet.route.length - 1] = actor;
    }

    function getActor(SubnetID calldata subnet) public pure returns (address) {
        if (subnet.route.length == 0) {
            return address(0);
        }

        return subnet.route[subnet.route.length - 1];
    }

    function isRoot(SubnetID calldata subnet) public pure returns (bool) {
        // gas-opt: original check: subnet.root > 0
        return subnet.route.length == 0 && subnet.root != 0;
    }

    function equals(SubnetID calldata subnet1, SubnetID calldata subnet2) public pure returns (bool) {
        if (subnet1.root != subnet2.root) {
            return false;
        }
        if (subnet1.route.length != subnet2.route.length) {
            return false;
        }

        return toHash(subnet1) == toHash(subnet2);
    }

    /// @notice Computes the common parent of the current subnet and the one given as argument
    function commonParent(SubnetID calldata subnet1, SubnetID calldata subnet2) public pure returns (SubnetID memory) {
        if (subnet1.root != subnet2.root) {
            return SubnetID({root: 0, route: new address[](0)});
        }

        uint256 i;
        uint256 subnet1routeLength = subnet1.route.length;
        uint256 subnet2routeLength = subnet2.route.length;
        while (i < subnet1routeLength && i < subnet2routeLength && subnet1.route[i] == subnet2.route[i]) {
            unchecked {
                ++i;
            }
        }
        if (i == 0) {
            return SubnetID({root: subnet1.root, route: new address[](0)});
        }

        address[] memory route = new address[](i);
        for (uint256 j; j < i; ) {
            route[j] = subnet1.route[j];
            unchecked {
                ++j;
            }
        }

        return SubnetID({root: subnet1.root, route: route});
    }

    /// @notice In the path determined by the current subnet id, it moves
    /// down in the path from the subnet id given as argument.
    /// subnet2 needs to be a prefix of the subnet1.
    /// If subnet1 is /a/b/c/d and subnet2 is /a/b, then the returned ID should be /a/b/c.
    /// @dev Revert will be triggered if subnet2 is an invalid input.
    function down(SubnetID calldata subnet1, SubnetID calldata subnet2) public pure returns (SubnetID memory) {
        if (subnet1.root != subnet2.root) {
            revert DifferentRootNetwork();
        }
        if (subnet1.route.length <= subnet2.route.length) {
            revert InvalidRoute();
        }

        uint256 i;
        uint256 subnet2routeLength = subnet2.route.length;
        while (i < subnet2routeLength && subnet1.route[i] == subnet2.route[i]) {
            unchecked {
                ++i;
            }
        }

        ++i;

        address[] memory route = new address[](i);

        for (uint256 j; j < i; ) {
            route[j] = subnet1.route[j];
            unchecked {
                ++j;
            }
        }

        return SubnetID({root: subnet1.root, route: route});
    }

    function isEmpty(SubnetID calldata subnetId) public pure returns (bool) {
        return subnetId.route.length == 0 && subnetId.root == 0;
    }
}
