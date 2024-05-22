// SPDX-License-Identifier: MIT
pragma solidity 0.8.23;

import "../src/USDCTest.sol";
import "./ConfigManager.sol";

contract DeployUSDCTest is ConfigManager {
    function run() external override {
        vm.startBroadcast();

        USDCTest erc20Token = new USDCTest();

        vm.stopBroadcast();

        writeConfig("OriginalToken", vm.toString(address(erc20Token)));
    }
}
