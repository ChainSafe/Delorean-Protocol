// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "../src/LinkedTokenReplica.sol";
import "../src/v2/LinkedTokenReplicaV2.sol";
import "./ConfigManager.sol";
import "@ipc/src/structs/Subnet.sol";
import "openzeppelin-contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract DeployIpcTokenReplica is ConfigManager {
    function deployIpcTokenReplica() external {
        vm.startBroadcast();
        LinkedTokenReplica initialImplementation = new LinkedTokenReplica();
        vm.stopBroadcast();

        // Log the address of the deployed contract implementation
        writeConfig("LinkedTokenReplicaImplementation", vm.toString(address(initialImplementation)));
    }

    function deployIpcTokenReplicaProxy(
        address initialImplementation,
        address gateway,
        address tokenContractAddress,
        uint64 _rootNetChainId,
        address[] memory _route,
        string memory token_name,
        string memory token_symbol,
        uint8 token_decimals
    ) external {
        vm.startBroadcast();

        SubnetID memory destinationSubnet = SubnetID({root: _rootNetChainId, route: _route});

        bytes memory initCall = abi.encodeCall(
            LinkedTokenReplica.initialize,
            (gateway, tokenContractAddress, destinationSubnet, address(0), token_name, token_symbol, token_decimals)
        );
        TransparentUpgradeableProxy transparentProxy = new TransparentUpgradeableProxy(
            initialImplementation,
            address(msg.sender),
            initCall
        );
        vm.stopBroadcast();
        writeConfig("LinkedTokenReplicaProxy", vm.toString(address(transparentProxy)));
    }

    function deployIpcTokenReplicaV2() external {
        vm.startBroadcast();
        LinkedTokenReplicaV2 initialImplementation = new LinkedTokenReplicaV2();
        vm.stopBroadcast();

        // Log the address of the deployed contract implementation
        writeConfig("LinkedTokenReplicaImplementation", vm.toString(address(initialImplementation)));
    }

    function upgradeIpcTokenReplica(
        address replicaProxy,
        address newReplicaImplementation,
        address gateway,
        address tokenContractAddress,
        uint64 _rootNetChainId,
        address[] memory _route,
        address controllerProxy,
        string memory token_name,
        string memory token_symbol,
        uint8 token_decimals
    ) external {
        SubnetID memory destinationSubnet = SubnetID({root: _rootNetChainId, route: _route});
        bytes memory initCall = abi.encodeCall(
            LinkedTokenReplicaV2.reinitialize,
            (
                gateway,
                tokenContractAddress,
                destinationSubnet,
                controllerProxy,
                token_name,
                token_symbol,
                token_decimals
            )
        );

        vm.startBroadcast();
        LinkedTokenReplica replica = LinkedTokenReplica(replicaProxy);
        replica.upgradeToAndCall(newReplicaImplementation, initCall);
        vm.stopBroadcast();
    }
}
