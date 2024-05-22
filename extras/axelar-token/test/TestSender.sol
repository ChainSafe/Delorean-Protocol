// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/IpcTokenSender.sol";
import "./DummyERC20.sol";
import { FvmAddressHelper } from "@ipc/src/lib/FvmAddressHelper.sol";

contract TestSender is Test {
    using FvmAddressHelper for address;

    function test_sender_Ok() public {

    }

    // TODO test_sender_fails_transfer (fails to transfer the tokens to itself)

    // TODO test_sender_fails_axelar (the Axelar gateway reverts)

}