// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "./CetfAPI.sol";

contract CetfExample {
    function enqueueTag(bytes32 tag) public returns (int256) {
        return CetfAPI.enqueueTag(tag);
    }
}
