// SPDX-License-Identifier: MIT

pragma solidity 0.8.23;

import {Script} from "forge-std/Script.sol";

contract ConfigManager is Script {
    string private configPath = "config.json"; // Path to your JSON config file

    function setOriginalToken(address originalToken) external {
        // Log the address of the deployed contract implementation
        writeConfig("OriginalToken", vm.toString(originalToken));
    }

    // Reads a value from the JSON config
    function readConfig(string memory key) internal returns (bytes memory value) {
        string memory path = string.concat(vm.projectRoot(), "/", configPath);
        require(vm.exists(path), "Config file does not exist.");
        string memory jsonData = vm.readFile(path);
        value = vm.parseJson(jsonData, key);
    }

    function readConfigAddress(string memory key) internal returns (address value) {
        string memory path = string.concat(vm.projectRoot(), "/", configPath);
        require(vm.exists(path), "Config file does not exist.");
        string memory json = vm.readFile(path);
        value = vm.parseJsonAddress(json, key);
    }

    // Writes a value to the JSON config
    function writeConfig(string memory key, string memory value) internal {
        // First, check if the file exists and read its contents
        string memory path = string.concat(vm.projectRoot(), "/", configPath);
        string memory jsonData;
        if (vm.exists(path)) {
            jsonData = vm.readFile(path);
        } else {
            // If the file doesn't exist, initialize an empty JSON object
            jsonData = '{"LinkedToken":{"OriginalToken":{}, "LinkedTokenReplicaProxy":{}, "LinkedTokenControllerProxy":{}, "LinkedTokenControllerImplementation":{}, "LinkedTokenReplicaImplementation":{}}}';
            vm.writeJson(jsonData, path);
        }

        vm.writeJson(value, path, string.concat(".LinkedToken.", key));
    }

    // Example usage within a script
    function run() external virtual {
        string memory exampleKey = "exampleKey";
        string memory exampleValue = "exampleValue";

        // Write a new value to the config writeConfig(exampleKey, exampleValue);

        // Read the value back from the config
        bytes memory retrievedValue = readConfig(exampleKey);
    }
}
