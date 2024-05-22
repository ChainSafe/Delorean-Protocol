// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/StdUtils.sol";
import "forge-std/StdCheats.sol";
import {CommonBase} from "forge-std/Base.sol";
import {GatewayDiamond} from "../../../src/GatewayDiamond.sol";
import {GatewayManagerFacet} from "../../../src/gateway/GatewayManagerFacet.sol";
import {GatewayFacetsHelper} from "../../helpers/GatewayFacetsHelper.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

uint256 constant ETH_SUPPLY = 129_590_000 ether;

contract GatewayActorHandler is CommonBase, StdCheats, StdUtils {
    using GatewayFacetsHelper for GatewayDiamond;
    GatewayManagerFacet managerFacet;

    uint256 private constant DEFAULT_MIN_VALIDATOR_STAKE = 10 ether;

    constructor(GatewayDiamond _gw) {
        managerFacet = _gw.manager();
        deal(address(this), ETH_SUPPLY);
    }

    function register(uint256 amount) public {
        amount = bound(amount, 0, 3 * DEFAULT_MIN_VALIDATOR_STAKE);
        managerFacet.register(amount);
    }

    function stake(uint256 amount) public {
        amount = bound(amount, 0, 3 * DEFAULT_MIN_VALIDATOR_STAKE);
        managerFacet.addStake{value: amount}();
    }

    function _pay(address to, uint256 amount) internal {
        (bool s, ) = to.call{value: amount}("");
        require(s, "pay() failed");
    }

    receive() external payable {}
}
