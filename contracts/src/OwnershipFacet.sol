// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import {LibDiamond} from "./lib/LibDiamond.sol";

contract OwnershipFacet {
    function transferOwnership(address _newOwner) external {
        LibDiamond.transferOwnership(_newOwner);
    }

    function owner() external view returns (address owner_) {
        owner_ = LibDiamond.contractOwner();
    }
}
