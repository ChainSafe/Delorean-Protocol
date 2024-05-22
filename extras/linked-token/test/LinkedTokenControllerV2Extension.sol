// SPDX-License-Identifier: MIT
pragma solidity 0.8.23;

import {LinkedTokenControllerV2} from "../src/v2/LinkedTokenControllerV2.sol";

contract LinkedTokenControllerV2Extension is LinkedTokenControllerV2 {
    function newFunctionReturns7() public returns (uint256) {
        return 7;
    }
}
