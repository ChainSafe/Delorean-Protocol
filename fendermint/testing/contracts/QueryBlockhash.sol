//SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

contract QueryBlockhash {
    constructor() {}

    function getBlockhash(uint blockNumber) public view returns (bytes32) {
        return blockhash(blockNumber);
    }
}
