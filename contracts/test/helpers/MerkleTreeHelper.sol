// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {Merkle} from "murky/Merkle.sol";

library MerkleTreeHelper {
    function createMerkleProofsForValidators(
        address[] memory addrs,
        uint256[] memory weight
    ) internal returns (bytes32, bytes32[][] memory) {
        Merkle merkleTree = new Merkle();

        if (addrs.length != weight.length) {
            revert("different array lengths");
        }
        uint256 len = addrs.length;

        bytes32 root;
        bytes32[][] memory proofs = new bytes32[][](len);
        bytes32[] memory data = new bytes32[](len);
        for (uint256 i = 0; i < len; i++) {
            data[i] = keccak256(bytes.concat(keccak256(abi.encode(addrs[i], weight[i]))));
        }

        root = merkleTree.getRoot(data);
        // get proof
        for (uint256 i = 0; i < len; i++) {
            bytes32[] memory proof = merkleTree.getProof(data, i);
            proofs[i] = proof;
        }

        return (root, proofs);
    }
}
