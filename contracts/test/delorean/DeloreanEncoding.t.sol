// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import "../../src/delorean/DeloreanAPI.sol";

contract DeloreanEncoding is Test {

    function test_encodingTagParams() public {        
        bytes memory encoded = DeloreanAPI.serializeEnqueueTagParams(DeloreanAPI.EnqueueTagParams(0x1111111111111111111111111111111111111111111111111111111111111111));
        console.logBytes(encoded);
    }

}