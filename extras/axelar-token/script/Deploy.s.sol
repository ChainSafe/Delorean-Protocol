// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import { Script, console2 as console } from "forge-std/Script.sol";
import "../src/IpcTokenHandler.sol";
import "../src/IpcTokenSender.sol";

contract Deploy is Script {
    function setUp() public {}

    function deployTokenHandler() public {
        string memory network = vm.envString("DEST_NETWORK");
        uint256 privateKey = vm.envUint(string.concat(network, "__PRIVATE_KEY"));

        console.log("deploying token handler to %s...", network);

        vm.startBroadcast(privateKey);
        IpcTokenHandler handler = new IpcTokenHandler({
            axelarIts: vm.envAddress(string.concat(network, "__AXELAR_ITS_ADDRESS")),
            ipcGateway: vm.envAddress(string.concat(network, "__IPC_GATEWAY_ADDRESS")),
            admin: vm.envAddress(string.concat(network, "__HANDLER_ADMIN_ADDRESS"))
        });
        vm.stopBroadcast();

        console.log("token handler deployed on %s: %s", network, address(handler));

        string memory path = string.concat(vm.projectRoot(), "/out/addresses.json");
        if (!vm.exists(path)) {
            vm.writeJson("{\"dest\":{\"token_handler\":{}},\"src\":{\"token_sender\":{}}}", path);
        }

        string memory key = "out";
        vm.serializeString(key, "network", network);
        string memory json = vm.serializeAddress(key, "token_handler", address(handler));
        vm.writeJson(json, path, ".dest");
    }

    function deployTokenSender() public {
        string memory originNetwork = vm.envString("ORIGIN_NETWORK");
        string memory destNetwork = vm.envString("DEST_NETWORK");
        uint256 privateKey = vm.envUint(string.concat(originNetwork, "__PRIVATE_KEY"));

        console.log("loading handler address...");

        string memory path = string.concat(vm.projectRoot(), "/out/addresses.json");
        require(vm.exists(path), "no addresses.json; please run DeployTokenHandler on the destination chain");

        string memory json = vm.readFile(path);
        address handlerAddr = vm.parseJsonAddress(json, ".dest.token_handler");
        console.log("handler address: %s", handlerAddr);

        console.log("deploying token sender to %s...", originNetwork);

        // Deploy the sender on Mumbai.
        vm.startBroadcast(privateKey);
        IpcTokenSender sender = new IpcTokenSender({
            axelarIts: vm.envAddress(string.concat(originNetwork, "__AXELAR_ITS_ADDRESS")),
            destinationChain: vm.envString(string.concat(destNetwork, "__AXELAR_CHAIN_NAME")),
            destinationTokenHandler: handlerAddr
        });
        vm.stopBroadcast();

        console.log("token sender deployed on %s: %s", originNetwork, address(sender));

        string memory key = "out";
        vm.serializeString(key, "network", originNetwork);
        json = vm.serializeAddress(key, "token_sender", address(sender));
        vm.writeJson(json, path, ".src");
    }
}
