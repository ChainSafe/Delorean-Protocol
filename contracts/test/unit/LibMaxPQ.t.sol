// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {Test} from "forge-std/Test.sol";
import {console} from "forge-std/console.sol";
import {MaxPQ, LibMaxPQ} from "../../src/lib/priority/LibMaxPQ.sol";
import {LibValidatorSet} from "../../src/lib/LibStaking.sol";
import {ValidatorSet} from "../../src/structs/Subnet.sol";

contract LibMaxPQTest is Test {
    using LibValidatorSet for ValidatorSet;
    using LibMaxPQ for MaxPQ;

    MaxPQ private maxPQ;
    ValidatorSet private validators;

    function setUp() public {
        validators.activeLimit = 50000;
    }

    function printMQ() internal view {
        uint16 size = maxPQ.getSize();
        for (uint16 i = 1; i <= size; i++) {
            address addr = maxPQ.inner.posToAddress[i];
            uint256 collateral = validators.getConfirmedCollateral(addr);
            console.log("idx", i, addr, collateral);
        }
    }

    function test_maxPQBasicInsert() public {
        require(maxPQ.getSize() == 0, "initial pq size not 0");

        address addr = address(1);
        validators.confirmDeposit(addr, 50);

        maxPQ.insert(validators, addr);

        require(maxPQ.getSize() == 1, "size not correct");
        (address maxAddress, uint256 maxValue) = maxPQ.max(validators);
        require(maxAddress == addr, "address not correct");
        require(maxValue == 50, "max collateral not correct");

        addr = address(2);
        validators.confirmDeposit(addr, 100);

        maxPQ.insert(validators, addr);

        require(maxPQ.getSize() == 2, "size not 2");
        (maxAddress, maxValue) = maxPQ.max(validators);
        require(maxAddress == addr, "address not 2");
        require(maxValue == 100, "max collateral not correct");
    }

    function test_maxPQBasicIncrease() public {
        require(maxPQ.getSize() == 0, "initial pq size not 0");

        address addr = address(1);
        validators.confirmDeposit(addr, 100);
        maxPQ.insert(validators, addr);

        addr = address(2);
        validators.confirmDeposit(addr, 50);
        maxPQ.insert(validators, addr);

        validators.confirmDeposit(address(2), 100);
        maxPQ.increaseReheapify(validators, addr);

        require(maxPQ.getSize() == 2, "size not 2");
        (address maxAddress, uint256 maxValue) = maxPQ.max(validators);
        require(maxAddress == address(2), "address not 2 after increase");
        require(maxValue == 150, "max collateral not 150");
    }

    function test_maxPQBasicDecrease() public {
        require(maxPQ.getSize() == 0, "initial pq size not 0");

        address addr = address(1);
        validators.confirmDeposit(addr, 100);
        maxPQ.insert(validators, addr);

        addr = address(2);
        validators.confirmDeposit(addr, 50);
        maxPQ.insert(validators, addr);

        validators.confirmWithdraw(address(1), 80);
        maxPQ.decreaseReheapify(validators, address(1));

        require(maxPQ.getSize() == 2, "size not 2");
        (address maxAddress, uint256 maxValue) = maxPQ.max(validators);
        require(maxAddress == address(2), "address not 2 after decrease");
        require(maxValue == 50, "max collateral not 50");
    }

    function test_maxPQBasicDelete() public {
        uint256 total = 100;
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));
            validators.confirmDeposit(addr, 100 * i);

            maxPQ.insert(validators, addr);
        }

        maxPQ.deleteReheapify(validators, address(10));

        uint256 maxValue = 100000000000000;
        for (uint256 i = total - 1; i > 0; i--) {
            (, uint256 nextMax) = maxPQ.max(validators);
            require(nextMax <= maxValue, "max collateral not correct");

            maxValue = nextMax;

            maxPQ.pop(validators);
        }
    }

    function test_maxPQInsertAndPop() public {
        require(maxPQ.getSize() == 0, "initial pq size not 0");

        uint256 total = 100;
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));
            validators.confirmDeposit(addr, 100 * i);
        }

        uint256 size = 1;
        address maxAddress;
        uint256 maxValue;

        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));

            maxPQ.insert(validators, addr);

            require(maxPQ.getSize() == size, "size not correct");
            (maxAddress, maxValue) = maxPQ.max(validators);

            require(maxAddress == addr, "address not correct");
            require(maxValue == 100 * i, "min collateral not correct");

            size++;
        }

        // printMQ();

        size = total;
        for (uint256 i = total; i > 0; i--) {
            address addr = address(uint160(i));

            (maxAddress, maxValue) = maxPQ.max(validators);
            require(maxAddress == addr, "address not correct");
            require(maxValue == 100 * i, "min collateral correct");
            require(maxPQ.getSize() == size, "size not correct");

            maxPQ.pop(validators);
            size--;
        }
    }

    function test_maxPQRandomInsertPop() public {
        require(maxPQ.getSize() == 0, "initial pq size not 0");

        uint256 total = 3000;

        uint256 valueStart = 1000000000000000000; // 1 ether
        uint256 valueRange = 1000000000000000000000000; // 1000000 ether
        for (uint256 i = 1; i <= total; i++) {
            address addr = address(uint160(i));
            uint256 value = (uint256(keccak256(abi.encode(addr, i))) % valueRange) + valueStart;
            validators.confirmDeposit(addr, value);

            maxPQ.insert(validators, addr);
        }

        uint256 maxValue = 2000000000000000000000000;
        for (uint256 i = total; i > 0; i--) {
            (, uint256 nextMax) = maxPQ.max(validators);
            require(nextMax <= maxValue, "min collateral not correct");

            maxValue = nextMax;

            maxPQ.pop(validators);
        }

        // printMQ();
    }
}
