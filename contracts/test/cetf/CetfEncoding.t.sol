// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import "../../src/cetf/CetfAPI.sol";

contract CetfEncoding is Test {

    function test_encodingTagParams() public {        
        bytes memory encoded = CetfAPI.serializeEnqueueTagParams(88);
        console.logBytes(encoded);
    }

}