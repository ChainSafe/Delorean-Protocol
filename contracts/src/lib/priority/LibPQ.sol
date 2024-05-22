// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {LibValidatorSet} from "../LibStaking.sol";
import {ValidatorSet} from "../../structs/Subnet.sol";
import {PQEmpty, PQDoesNotContainAddress} from "../../errors/IPCErrors.sol";

/// The implementation that mimics the Java impl in https://algs4.cs.princeton.edu/24pq/IndexMinPQ.java.html.

/// The inner data structure for both min and max priority queue
struct PQ {
    /// The size of the priority queue
    uint16 size;
    /// @notice The mapping from validator address to the position on the priority queue. Position is 1-based indexing.
    mapping(address => uint16) addressToPos;
    /// @notice The mapping from the position on the priority queue to validator address.
    mapping(uint16 => address) posToAddress;
}

library LibPQ {
    using LibValidatorSet for ValidatorSet;

    function isEmpty(PQ storage self) internal view returns (bool) {
        return self.size == 0;
    }

    function requireNotEmpty(PQ storage self) internal view {
        if (self.size == 0) {
            revert PQEmpty();
        }
    }

    function getSize(PQ storage self) internal view returns (uint16) {
        return self.size;
    }

    function contains(PQ storage self, address validator) internal view returns (bool) {
        return self.addressToPos[validator] != 0;
    }

    function getPosOrRevert(PQ storage self, address validator) internal view returns (uint16 pos) {
        pos = self.addressToPos[validator];
        if (pos == 0) {
            revert PQDoesNotContainAddress();
        }
    }

    function del(PQ storage self, uint16 pos) internal {
        address addr = self.posToAddress[pos];
        delete self.posToAddress[pos];
        delete self.addressToPos[addr];
    }

    function getPower(
        PQ storage self,
        ValidatorSet storage validators,
        uint16 pos
    ) internal view returns (uint256) {
        address addr = self.posToAddress[pos];
        return validators.getPower(addr);
    }

    function getConfirmedCollateral(
        PQ storage self,
        ValidatorSet storage validators,
        uint16 pos
    ) internal view returns (uint256) {
        address addr = self.posToAddress[pos];
        return validators.getConfirmedCollateral(addr);
    }

    function exchange(PQ storage self, uint16 pos1, uint16 pos2) internal {
        assert(pos1 <= self.size);
        assert(pos2 <= self.size);

        address addr1 = self.posToAddress[pos1];
        address addr2 = self.posToAddress[pos2];

        self.addressToPos[addr1] = pos2;
        self.addressToPos[addr2] = pos1;

        self.posToAddress[pos2] = addr1;
        self.posToAddress[pos1] = addr2;
    }
}