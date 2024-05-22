// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import { Script, console2 as console } from "forge-std/Script.sol";
import { IERC20 } from "openzeppelin-contracts/interfaces/IERC20.sol";
import { SubnetID } from "@ipc/src/structs/Subnet.sol";

import "../src/IpcTokenSender.sol";

contract Deposit is Script {
    function setUp() public {}

    function run(bytes32 tokenId, uint256 amount, uint256 gasPayment, address beneficiary, uint64 subnetRoot, address subnetAddr) public {
        IERC20 token;
        uint256 privateKey;

        {
            string memory network = vm.envString("ORIGIN_NETWORK");
            token = IERC20(vm.envAddress(string.concat(network, "__ORIGIN_TOKEN_ADDRESS")));
            privateKey = vm.envUint(string.concat(network, "__PRIVATE_KEY"));
        }

        address senderAddr;
        {
            string memory path = string.concat(vm.projectRoot(), "/out/addresses.json");
            require(vm.exists(path), "no addresses.json; run the deploy targets");
            string memory json = vm.readFile(path);
            senderAddr = vm.parseJsonAddress(json, ".src.token_sender");
        }

        console.log("token sender address: %s", senderAddr);

        SubnetID memory subnetId;
        {
            address[] memory route = new address[](1);
            route[0] = subnetAddr;
            subnetId = SubnetID({root: subnetRoot, route: route});
        }

        console.log("approving amount in origin token @ %s: %d", address(token), amount);
        vm.startBroadcast(privateKey);
        token.approve(senderAddr, amount);
        IpcTokenSender(senderAddr).fundSubnet{value: gasPayment}({tokenId: tokenId, subnet: subnetId, recipient: beneficiary, amount: amount});

        vm.stopBroadcast();
    }
}
