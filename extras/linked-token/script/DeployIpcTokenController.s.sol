// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "./ConfigManager.sol";
import "../src/LinkedTokenController.sol";
import "../src/v2/LinkedTokenControllerV2.sol";
import "@ipc/src/structs/Subnet.sol";
import "openzeppelin-contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract DeployIpcTokenController is ConfigManager {
    function deployIpcTokenController() external {
        vm.startBroadcast();
        LinkedTokenController initialImplementation = new LinkedTokenController();
        vm.stopBroadcast();

        // Log the address of the deployed contract implementation
        writeConfig("LinkedTokenControllerImplementation", vm.toString(address(initialImplementation)));
    }

    function deployIpcTokenControllerProxy(
        address initialImplementation,
        address gateway,
        address tokenContractAddress,
        uint64 _rootNetChainId,
        address[] memory _route
    ) external {
        vm.startBroadcast();

        // Example for setting up the SubnetID, adjust according to your actual setup
        SubnetID memory destinationSubnet = SubnetID({root: _rootNetChainId, route: _route});

        bytes memory initCall = abi.encodeCall(
            LinkedTokenController.initialize,
            (gateway, tokenContractAddress, destinationSubnet, address(0))
        );
        TransparentUpgradeableProxy transparentProxy = new TransparentUpgradeableProxy(
            initialImplementation,
            address(msg.sender),
            initCall
        );
        vm.stopBroadcast();
        writeConfig("LinkedTokenControllerProxy", vm.toString(address(transparentProxy)));
    }

    function deployIpcTokenControllerV2() external {
        vm.startBroadcast();
        LinkedTokenControllerV2 initialImplementation = new LinkedTokenControllerV2();
        vm.stopBroadcast();

        // Log the address of the deployed contract implementation
        writeConfig("LinkedTokenControllerImplementation", vm.toString(address(initialImplementation)));
    }

    function upgradeIpcTokenController(
        address controllerProxy,
        address newControllerImplementation,
        address gateway,
        address tokenContractAddress,
        uint64 _rootNetChainId,
        address[] memory _route,
        address replicaProxy
    ) external {
        SubnetID memory destinationSubnet = SubnetID({root: _rootNetChainId, route: _route});
        bytes memory initCall = abi.encodeCall(
            LinkedTokenControllerV2.reinitialize,
            (gateway, tokenContractAddress, destinationSubnet, replicaProxy)
        );

        vm.startBroadcast();
        LinkedTokenController controller = LinkedTokenController(controllerProxy);
        controller.upgradeToAndCall(newControllerImplementation, initCall);
        vm.stopBroadcast();
    }
}
