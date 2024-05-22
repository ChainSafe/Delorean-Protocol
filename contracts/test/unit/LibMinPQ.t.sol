// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {Test} from "forge-std/Test.sol";
import {console} from "forge-std/console.sol";
import {MinPQ, LibMinPQ} from "../../src/lib/priority/LibMinPQ.sol";
import {LibValidatorSet} from "../../src/lib/LibStaking.sol";
import {ValidatorSet} from "../../src/structs/Subnet.sol";

contract LibMinPQTest is Test {
    using LibValidatorSet for ValidatorSet;
    using LibMinPQ for MinPQ;

    MinPQ private minPQ;
    ValidatorSet private validators;

    function setUp() public {
        validators.activeLimit = 50000;
    }

    function printMQ() internal view {
        uint16 size = minPQ.getSize();
        for (uint16 i = 1; i <= size; i++) {
            address addr = minPQ.inner.posToAddress[i];
            uint256 collateral = validators.getConfirmedCollateral(addr);
            console.log("idx", i, addr, collateral);
        }
    }

    function test_minPQBasicInsert() public {
        require(minPQ.getSize() == 0, "initial pq size not 0");

        address addr = address(1);
        validators.confirmDeposit(addr, 100);

        minPQ.insert(validators, addr);

        require(minPQ.getSize() == 1, "size not correct");
        (address minAddress, uint256 minValue) = minPQ.min(validators);
        require(minAddress == addr, "address not correct");
        require(minValue == 100, "min collateral correct");

        addr = address(2);
        validators.confirmDeposit(addr, 50);

        minPQ.insert(validators, addr);

        require(minPQ.getSize() == 2, "size not 2");
        (minAddress, minValue) = minPQ.min(validators);
        require(minAddress == addr, "address not 2");
        require(minValue == 50, "min collateral 50");
    }

    function test_minPQBasicIncrease() public {
        require(minPQ.getSize() == 0, "initial pq size not 0");

        address addr = address(1);
        validators.confirmDeposit(addr, 100);
        minPQ.insert(validators, addr);

        addr = address(2);
        validators.confirmDeposit(addr, 50);
        minPQ.insert(validators, addr);

        validators.confirmDeposit(address(2), 100);
        minPQ.increaseReheapify(validators, addr);

        require(minPQ.getSize() == 2, "size not 2");
        (address minAddress, uint256 minValue) = minPQ.min(validators);
        require(minAddress == address(1), "address not 1 after increase");
        require(minValue == 100, "min collateral not 100");
    }

    function test_minPQBasicDecrease() public {
        require(minPQ.getSize() == 0, "initial pq size not 0");

        address addr = address(1);
        validators.confirmDeposit(addr, 100);
        minPQ.insert(validators, addr);

        addr = address(2);
        validators.confirmDeposit(addr, 50);
        minPQ.insert(validators, addr);

        validators.confirmWithdraw(address(1), 80);
        minPQ.decreaseReheapify(validators, address(1));

        require(minPQ.getSize() == 2, "size not 2");
        (address minAddress, uint256 minValue) = minPQ.min(validators);
        require(minAddress == address(1), "address not 1 after decrease");
        require(minValue == 20, "min collateral not 20");
    }

    function test_minPQBasicDelete() public {
        uint256 total = 100;
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));
            validators.confirmDeposit(addr, 100 * i);

            minPQ.insert(validators, addr);
        }

        minPQ.deleteReheapify(validators, address(10));
        require(!minPQ.contains(address(10)), "delete does not work");

        uint256 minValue = 0;
        for (uint256 i = total - 1; i > 0; i--) {
            (, uint256 nextMin) = minPQ.min(validators);
            require(nextMin >= minValue, "min collateral not correct");

            minValue = nextMin;

            minPQ.pop(validators);
        }
    }

    function test_minPQInsertAndPop() public {
        require(minPQ.getSize() == 0, "initial pq size not 0");

        uint256 total = 100;
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));
            validators.confirmDeposit(addr, 100 * i);
        }

        uint256 size = 1;
        address minAddress;
        uint256 minValue;

        for (uint256 i = total; i > 0; i--) {
            address addr = address(uint160(i));

            minPQ.insert(validators, addr);

            require(minPQ.getSize() == size, "size not correct");
            (minAddress, minValue) = minPQ.min(validators);

            require(minAddress == addr, "address not correct");
            require(minValue == 100 * i, "min collateral not correct");

            size++;
        }

        // printMQ();

        size = total;
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));

            (minAddress, minValue) = minPQ.min(validators);
            require(minAddress == addr, "address not correct");
            require(minValue == 100 * i, "min collateral correct");
            require(minPQ.getSize() == size, "size not correct");

            minPQ.pop(validators);
            size--;
        }
    }

    function test_minPQRandomInsertPop() public {
        require(minPQ.getSize() == 0, "initial pq size not 0");

        uint256 total = 3000;

        uint256 valueStart = 1000000000000000000; // 1 ether
        uint256 valueRange = 1000000000000000000000000; // 1000000 ether
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));
            uint256 value = (uint256(keccak256(abi.encode(addr, i))) % valueRange) + valueStart;
            validators.confirmDeposit(addr, value);

            minPQ.insert(validators, addr);
        }

        uint256 minValue = 0;
        for (uint256 i = total; i > 0; i--) {
            (, uint256 nextMin) = minPQ.min(validators);
            require(nextMin >= minValue, "min collateral not correct");

            minValue = nextMin;

            minPQ.pop(validators);
        }

        // printMQ();
    }
}
