// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import {MerkleTreeHelper} from "../helpers/MerkleTreeHelper.sol";
import {MerkleProof} from "openzeppelin-contracts/utils/cryptography/MerkleProof.sol";
import {Merkle} from "murky/Merkle.sol";

contract MerkleTree is Test {
    Merkle merkleTree;

    function test_merkle_proofInterface() public {
        merkleTree = new Merkle();

        address[] memory addrs = new address[](4);
        addrs[0] = vm.addr(1939);
        addrs[1] = vm.addr(1101);
        addrs[2] = vm.addr(4323);
        addrs[3] = vm.addr(3431);

        uint256[] memory weights = new uint256[](4);
        weights[0] = 234;
        weights[1] = 14;
        weights[2] = 24;
        weights[3] = 433;

        (bytes32 root, bytes32[][] memory proofs) = MerkleTreeHelper.createMerkleProofsForValidators(addrs, weights);

        bool valid;
        bytes32 leaf;

        leaf = keccak256(bytes.concat(keccak256(abi.encode(addrs[0], weights[0]))));
        valid = MerkleProof.verify({proof: proofs[0], root: root, leaf: leaf});
        require(valid, "the valid leaf in the tree");

        leaf = keccak256(bytes.concat(keccak256(abi.encode(addrs[0], weights[1]))));
        valid = MerkleProof.verify({proof: proofs[0], root: root, leaf: leaf});
        require(!valid, "invalid leaf is not in the tree");
    }
}
