// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "./CetfAPI.sol";

contract CetfExample {
    function releaseKey(bytes32 tag) external returns (int256) {
        int256 result = CetfAPI.enqueueTag(tag);
        return (result);
    }
}
