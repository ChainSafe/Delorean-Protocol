// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";

import "fevmate/utils/FilAddress.sol";
import "../../src/lib/AccountHelper.sol";

contract AccountHelperTest is Test {
    using AccountHelper for address;

    function test_IsSystemActor_True() public pure {
        require(FilAddress.SYSTEM_ACTOR.isSystemActor() == true);
    }

    function test_IsSystemActor_False() public pure {
        require(vm.addr(1234).isSystemActor() == false);
    }

    function activateAccount(address account) internal {
        vm.deal(account, 1 ether);
    }
}
