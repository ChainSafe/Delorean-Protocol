// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/StdUtils.sol";
import "forge-std/StdCheats.sol";
import {CommonBase} from "forge-std/Base.sol";
import {SubnetActorDiamond} from "../../../src/SubnetActorDiamond.sol";
import {SubnetActorGetterFacet} from "../../../src/subnet/SubnetActorGetterFacet.sol";
import {SubnetActorMock} from "../../mocks/SubnetActorMock.sol";
import {TestUtils} from "../../helpers/TestUtils.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

uint256 constant ETH_SUPPLY = 129_590_000 ether;

contract SubnetActorHandler is CommonBase, StdCheats, StdUtils {
    using EnumerableSet for EnumerableSet.AddressSet;

    SubnetActorMock private managerFacet;
    SubnetActorGetterFacet private getterFacet;

    uint256 private constant DEFAULT_MIN_VALIDATOR_STAKE = 10 ether;

    // Ghost variables.

    // All validators: waiting and active.
    // A validator is added, if it is called `join` successfully.
    EnumerableSet.AddressSet private ghost_validators;
    mapping(address => uint256) public ghost_validators_staked;
    mapping(address => uint256) public ghost_validators_unstaked;

    uint256 public ghost_stakedSum;
    uint256 public ghost_unstakedSum;

    constructor(SubnetActorDiamond _subnetActor) {
        managerFacet = SubnetActorMock(address(_subnetActor));
        getterFacet = SubnetActorGetterFacet(address(_subnetActor));

        deal(address(this), ETH_SUPPLY);
    }

    /// getRandomValidator returns a validator from the known validators with probability about 20 %,
    /// otherwise it returns a random validator address generated from id.
    /// It can't return address(0);
    function getRandomValidator(uint8 id) public view returns (address) {
        address addr;
        if (id < 200) {
            addr = getRandomValidatorFromSetOrZero(id);
        } else {
            (addr, ) = TestUtils.deriveValidatorAddress(id);
        }
        if (addr == address(0)) {
            return msg.sender;
        }
        return addr;
    }

    function getRandomValidatorFromSetOrZero(uint8 seed) public view returns (address) {
        uint256 length = ghost_validators.length();
        if (length == 0) {
            return address(0);
        }
        return ghost_validators.values()[seed % length];
    }

    function joinedValidatorsNumber() public view returns (uint256) {
        return ghost_validators.values().length;
    }

    function joinedValidators() public view returns (address[] memory) {
        return ghost_validators.values();
    }

    function join(uint8 id, uint256 amount) public {
        if (id == 0) {
            return;
        }
        amount = bound(amount, 0, 3 * DEFAULT_MIN_VALIDATOR_STAKE);

        (address validator, bytes memory publicKey) = TestUtils.deriveValidatorAddress(id);

        _pay(validator, amount);
        vm.prank(validator);
        managerFacet.join{value: amount}(publicKey);
        managerFacet.confirmNextChange();

        ghost_stakedSum += amount;
        ghost_validators_staked[validator] += amount;
        ghost_validators.add(validator);
    }

    function stake(uint8 id, uint256 amount) public {
        amount = bound(amount, 0, 3 * DEFAULT_MIN_VALIDATOR_STAKE);
        address validator = getRandomValidator(id);
        _pay(validator, amount);

        vm.prank(validator);
        managerFacet.stake{value: amount}();
        managerFacet.confirmNextChange();

        ghost_stakedSum += amount;
        ghost_validators_staked[validator] += amount;
    }

    function unstake(uint8 id, uint256 amount) public {
        amount = bound(amount, 0, 3 * DEFAULT_MIN_VALIDATOR_STAKE);
        address validator = getRandomValidator(id);

        vm.prank(validator);
        managerFacet.unstake(amount);
        managerFacet.confirmNextChange();

        ghost_unstakedSum += amount;
        ghost_validators_unstaked[validator] += amount;
    }

    function leave(uint8 id) public returns (address) {
        address validator = getRandomValidatorFromSetOrZero(id);
        if (validator == address(0)) {
            return validator;
        }

        uint256 amount = getterFacet.getTotalValidatorCollateral(validator);

        vm.prank(validator);
        managerFacet.leave();
        managerFacet.confirmNextChange();

        ghost_validators.remove(validator);
        ghost_validators_unstaked[validator] = amount;
        ghost_unstakedSum += amount;

        return validator;
    }

    function _pay(address to, uint256 amount) internal {
        (bool s, ) = to.call{value: amount}("");
        require(s, "pay() failed");
    }

    receive() external payable {}
}
