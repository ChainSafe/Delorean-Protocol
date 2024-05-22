// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {Test} from "forge-std/Test.sol";
import {MaxPQ, LibMaxPQ} from "../../src/lib/priority/LibMaxPQ.sol";
import {MinPQ, LibMinPQ} from "../../src/lib/priority/LibMinPQ.sol";
import {LibValidatorSet} from "../../src/lib/LibStaking.sol";
import {ValidatorSet} from "../../src/structs/Subnet.sol";

contract LibValidatorSetTest is Test {
    using LibValidatorSet for ValidatorSet;
    using LibMaxPQ for MaxPQ;
    using LibMinPQ for MinPQ;

    ValidatorSet private validators;

    function test_validatorSet_noPriorActiveValidators() public {
        validators.activeLimit = 2;

        validators.confirmDeposit(address(1), 50);
        validators.confirmDeposit(address(2), 100);

        require(validators.isActiveValidator(address(1)), "address 1 should be active");
        require(validators.isActiveValidator(address(2)), "address 2 should be active");
        require(validators.waitingValidators.getSize() == 0, "waiting validators should be empty");
        require(validators.activeValidators.getSize() == 2, "active validators size should be 2");
    }

    function test_validatorSet_activeValidatorDepositCollateral() public {
        validators.activeLimit = 100;

        for (uint160 i = 1; i <= 100; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }
        require(validators.waitingValidators.getSize() == 0, "waiting validators should be empty");
        require(validators.activeValidators.getSize() == 100, "active validators size should be 100");

        validators.confirmDeposit(address(1), 100);

        for (uint160 i = 1; i < 100; i++) {
            validators.activeValidators.pop(validators);
        }

        (address maxAddress, uint256 maxCollateral) = validators.activeValidators.min(validators);
        require(maxAddress == address(1), "address 1 should be the last validator");
        require(maxCollateral == 101, "address 1 collateral incorrect");
    }

    /// @notice Exceeding active validator limit and there is no waiting validators
    /// Setup: 100 active validators with collateral less than 101. New validator
    ///        deposits 101.
    /// Expected: 100 active validators, max active deposit is 101. 1 waiting validator, collateral 1
    function test_validatorSet_exceedingActiveLimitNoWaiting() public {
        validators.activeLimit = 100;

        for (uint160 i = 1; i <= 100; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        validators.confirmDeposit(address(101), 101);

        /// check waiting validator
        require(!validators.activeValidators.contains(address(1)), "address 1 should not be active");
        require(validators.waitingValidators.contains(address(1)), "address 1 should be waiting");
        require(validators.waitingValidators.getSize() == 1, "waiting validators should only have 1");

        // check new active validators
        for (uint160 i = 1; i < 100; i++) {
            validators.activeValidators.pop(validators);
        }

        (address maxAddress, uint256 maxCollateral) = validators.activeValidators.min(validators);
        require(maxAddress == address(101), "address 101 should be the last validator");
        require(maxCollateral == 101, "address 101 collateral not 101");
    }

    /// @notice Exceeding active validator limit and there are waiting validators
    /// Setup: 100 active validators with collateral btw 2 to 101. 5 waiting validators with collateral 1.
    ///        Incoming validator is not active validator nor waiting validator with collateral 1000
    /// Expected: 100 active validators, max active deposit is 1000. 6 waiting validator, collateral 1 or 2
    function test_validatorSet_exceedingActiveLimitWithWaitingI() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 2; i <= 101; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 102; i <= 106; i++) {
            validators.confirmDeposit(address(i), 1);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        // new validator
        validators.confirmDeposit(address(1000), 1000);

        // ============= check expected result ==========

        /// check waiting validator
        require(validators.waitingValidators.getSize() == 6, "waiting validators should only have 6");
        require(!validators.activeValidators.contains(address(2)), "address 2 should not be active");
        require(validators.waitingValidators.contains(address(2)), "address 2 should be waiting");

        (address maxAddress, uint256 maxCollateral) = validators.waitingValidators.max(validators);
        require(maxAddress == address(2), "address 2 should be the max collateral validtor");
        require(maxCollateral == 2, "address 2 should have max collateral 2");

        // check new active validators
        for (uint160 i = 1; i < 100; i++) {
            validators.activeValidators.pop(validators);
        }
        (maxAddress, maxCollateral) = validators.activeValidators.min(validators);
        require(maxAddress == address(1000), "address 1000 should be the last validator");
        require(maxCollateral == 1000, "address 1000 collateral not 1000");
    }

    /// @notice Exceeding active validator limit and there are waiting validators
    /// Setup: 100 active validators with collateral btw 2 to 101. 5 waiting validators with collateral 1.
    ///        Incoming validator is a waiting validator with collateral 1000
    /// Expected: 100 active validators, max active deposit is 1000. 5 waiting validator, collateral 1 or 2
    function test_validatorSet_exceedingActiveLimitWithWaitingII() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 2; i <= 101; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 102; i <= 106; i++) {
            validators.confirmDeposit(address(i), 1);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        // waiting validator makes deposit
        validators.confirmDeposit(address(102), 1000);

        // ============= check expected result ==========

        /// check waiting validator
        require(validators.waitingValidators.getSize() == 5, "waiting validators should only have 5");
        require(!validators.activeValidators.contains(address(2)), "address 2 should not be active");
        require(validators.waitingValidators.contains(address(2)), "address 2 should be waiting");
        require(!validators.waitingValidators.contains(address(102)), "address 102 should not be waiting");
        require(validators.activeValidators.contains(address(102)), "address 102 should be active");

        (address maxAddress, uint256 maxCollateral) = validators.waitingValidators.max(validators);
        require(maxAddress == address(2), "address 2 should be the max collateral validtor");
        require(maxCollateral == 2, "address 2 should have max collateral 2");

        for (uint160 i = 106; i >= 103; i--) {
            validators.waitingValidators.pop(validators);
            (maxAddress, maxCollateral) = validators.waitingValidators.max(validators);
            require(uint160(maxAddress) <= 106, "address too big");
            require(uint160(maxAddress) >= 103, "address too small");
            require(maxCollateral == 1, "should have max collateral 2");
        }

        // check new active validators
        for (uint160 i = 1; i < 100; i++) {
            validators.activeValidators.pop(validators);
        }
        (maxAddress, maxCollateral) = validators.activeValidators.min(validators);
        require(maxAddress == address(102), "address 1000 should be the last validator");
        require(maxCollateral == 1001, "address 102 collateral not 1001");
    }

    /// @notice Exceeding active validator limit and there are waiting validators
    /// Setup: 100 active validators with collateral btw 2 to 101. 5 waiting validators with collateral 1.
    ///        Incoming validator is not active nor waiting validator with collateral 1
    /// Expected: 100 active validators no change. 6 waiting validator, collateral 1.
    function test_validatorSet_exceedingActiveLimitWithWaitingIII() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 2; i <= 101; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 102; i <= 106; i++) {
            validators.confirmDeposit(address(i), 1);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        // waiting validator makes deposit
        validators.confirmDeposit(address(107), 1);

        // ============= check expected result ==========

        /// check waiting validator
        require(validators.waitingValidators.getSize() == 6, "waiting validators should only have 6");
        for (uint160 i = 102; i <= 107; i++) {
            require(!validators.isActiveValidator(address(i)), "address should not be active");
            require(validators.waitingValidators.contains(address(i)), "address should be waiting");
        }

        for (uint160 i = 107; i >= 102; i--) {
            (address maxAddress, uint256 maxCollateral) = validators.waitingValidators.max(validators);
            validators.waitingValidators.pop(validators);
            require(uint160(maxAddress) <= 107, "address too big");
            require(uint160(maxAddress) >= 102, "address too small");
            require(maxCollateral == 1, "should have max collateral 2");
        }

        // check active validators no change
        require(validators.activeValidators.getSize() == 100, "active validators should only have 100");
        for (uint160 i = 2; i <= 101; i++) {
            require(validators.isActiveValidator(address(i)), "address should still be active");
        }
    }

    /// @notice Exceeding active validator limit and there are waiting validators
    /// Setup: 100 active validators with collateral btw 3 to 102. 5 waiting validators with collateral 1.
    ///        Incoming validator is waiting validator with new collateral 2
    /// Expected: 100 active validators no change. 5 waiting validator, collateral 1 or 2.
    function test_validatorSet_exceedingActiveLimitWithWaitingIV() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 3; i <= 102; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 103; i <= 107; i++) {
            validators.confirmDeposit(address(i), 1);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        // waiting validator makes withdraw
        validators.confirmDeposit(address(107), 1);

        // ============= check expected result ==========

        /// check waiting validator
        require(validators.waitingValidators.getSize() == 5, "waiting validators should only have 5");
        for (uint160 i = 103; i <= 107; i++) {
            require(!validators.isActiveValidator(address(i)), "address should not be active");
            require(validators.waitingValidators.contains(address(i)), "address should be waiting");
        }

        (address maxAddress, uint256 maxCollateral) = validators.waitingValidators.max(validators);
        require(maxAddress <= address(107), "max waiting validator should be 107");
        require(maxCollateral == 2, "max collateral of waiting should be 2");

        for (uint160 i = 106; i >= 103; i--) {
            validators.waitingValidators.pop(validators);

            (maxAddress, maxCollateral) = validators.waitingValidators.max(validators);
            require(uint160(maxAddress) <= 106, "address too big");
            require(uint160(maxAddress) >= 103, "address too small");
            require(maxCollateral == 1, "should have max collateral 2");
        }

        // check active validators no change
        require(validators.activeValidators.getSize() == 100, "active validators should only have 100");
        for (uint160 i = 3; i <= 102; i++) {
            require(validators.isActiveValidator(address(i)), "address should still be active");
        }
    }

    function test_validatorSet_waitingVadalitorLeaves() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 3; i <= 102; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 103; i <= 107; i++) {
            validators.confirmDeposit(address(i), 1);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        validators.confirmWithdraw(address(107), 1);

        // ============= check expected result ==========

        /// check waiting validator
        require(!validators.waitingValidators.contains(address(107)), "address 107 should not be waiting");

        require(validators.waitingValidators.getSize() == 4, "waiting validators should only have 4");
        for (uint160 i = 103; i <= 106; i++) {
            require(!validators.isActiveValidator(address(i)), "address should not be active");
            require(validators.waitingValidators.contains(address(i)), "address should be waiting");
        }

        for (uint160 i = 106; i >= 103; i--) {
            (address maxAddress, uint256 maxCollateral) = validators.waitingValidators.max(validators);
            require(uint160(maxAddress) <= 106, "address too big");
            require(uint160(maxAddress) >= 103, "address too small");
            require(maxCollateral == 1, "should have max collateral 2");

            validators.waitingValidators.pop(validators);
        }

        // check active validators no change
        require(validators.activeValidators.getSize() == 100, "active validators should only have 100");
        for (uint160 i = 3; i <= 102; i++) {
            require(validators.isActiveValidator(address(i)), "address should still be active");
        }
    }

    function test_validatorSet_waitingVadalitorReduceCollateral() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 3; i <= 102; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 103; i <= 107; i++) {
            validators.confirmDeposit(address(i), 2);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        // waiting validator makes deposit
        validators.confirmWithdraw(address(107), 1);

        // ============= check expected result ==========

        /// check waiting validator
        require(validators.waitingValidators.getSize() == 5, "waiting validators should only have 5");
        for (uint160 i = 103; i <= 107; i++) {
            require(!validators.isActiveValidator(address(i)), "address should not be active");
            require(validators.waitingValidators.contains(address(i)), "address should be waiting");
        }

        address maxAddress;
        uint256 maxCollateral;

        for (uint160 i = 106; i >= 103; i--) {
            (maxAddress, maxCollateral) = validators.waitingValidators.max(validators);
            require(uint160(maxAddress) <= 106, "address too big");
            require(uint160(maxAddress) >= 103, "address too small");
            require(maxCollateral == 2, "should have max collateral 2");

            validators.waitingValidators.pop(validators);
        }

        (maxAddress, maxCollateral) = validators.waitingValidators.max(validators);
        require(uint160(maxAddress) == 107, "address 107 should be min");
        require(maxCollateral == 1, "address 107 should have max collateral 1");

        // check active validators no change
        require(validators.activeValidators.getSize() == 100, "active validators should only have 100");
        for (uint160 i = 3; i <= 102; i++) {
            require(validators.isActiveValidator(address(i)), "address should still be active");
        }
    }

    function test_validatorSet_activeVadalitorLeavesNoWaiting() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 1; i <= 100; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        validators.confirmWithdraw(address(1), 1);

        /// check waiting validator
        require(validators.activeValidators.getSize() == 99, "active validators should only have 99");
        require(validators.waitingValidators.getSize() == 0, "waiting validators should only have 0");
    }

    function test_validatorSet_activeVadalitorLeavesWithWaiting() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 1; i <= 110; i++) {
            validators.confirmDeposit(address(i), i);
        }

        validators.confirmWithdraw(address(11), 11);

        require(!validators.isActiveValidator(address(11)), "address 11 should not be active");
        require(!validators.waitingValidators.contains(address(11)), "address 11 should not be waiting");

        require(validators.isActiveValidator(address(10)), "address 10 should be active");
        require(!validators.waitingValidators.contains(address(10)), "address 10 should not be waiting");

        require(validators.activeValidators.getSize() == 100, "active validators should only have 100");
        require(validators.waitingValidators.getSize() == 9, "waiting validators should only have 9");

        (address maxAddress, uint256 maxCollateral) = validators.waitingValidators.max(validators);
        require(maxAddress == address(9), "max waiting validator should be address 9");
        require(maxCollateral == 9, "max waiting validator collateral should be 9");

        (address minAddress, uint256 minCollateral) = validators.activeValidators.min(validators);
        require(minAddress == address(10), "min active validator should be address 10");
        require(minCollateral == 10, "min active validator collateral should be 10");
    }

    function test_validatorSet_activeVadalitorWithdrawsWaitingTooSmall() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 3; i <= 102; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 103; i <= 107; i++) {
            validators.confirmDeposit(address(i), 2);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        validators.confirmWithdraw(address(11), 1);

        require(validators.isActiveValidator(address(11)), "address 11 should still be active");
    }

    function test_validatorSet_activeVadalitorWithdrawsWaitingPromoted() public {
        // setup
        validators.activeLimit = 100;

        for (uint160 i = 3; i <= 102; i++) {
            validators.confirmDeposit(address(i), i);
            require(validators.isActiveValidator(address(i)), "address should be active");
        }

        for (uint160 i = 103; i <= 107; i++) {
            validators.confirmDeposit(address(i), 2);
            require(!validators.isActiveValidator(address(i)), "address should not be active");
        }

        validators.confirmWithdraw(address(11), 10);

        require(!validators.isActiveValidator(address(11)), "address 11 should not be active");
        require(validators.waitingValidators.contains(address(11)), "address 11 should be waiting");

        require(validators.isActiveValidator(address(10)), "address 10 should be active");
        require(!validators.waitingValidators.contains(address(10)), "address 10 should not be waiting");
    }
}
