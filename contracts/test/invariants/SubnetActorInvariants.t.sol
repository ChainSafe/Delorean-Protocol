// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "../../src/errors/IPCErrors.sol";
import {StdInvariant} from "forge-std/Test.sol";
import {SubnetID, Subnet} from "../../src/structs/Subnet.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";
import {GatewayDiamond} from "../../src/GatewayDiamond.sol";
import {SubnetActorDiamond} from "../../src/SubnetActorDiamond.sol";
import {GatewayDiamond} from "../../src/GatewayDiamond.sol";
import {GatewayGetterFacet} from "../../src/gateway/GatewayGetterFacet.sol";
import {GatewayMessengerFacet} from "../../src/gateway/GatewayMessengerFacet.sol";
import {GatewayManagerFacet} from "../../src/gateway/GatewayManagerFacet.sol";
import {SubnetActorHandler, ETH_SUPPLY} from "./handlers/SubnetActorHandler.sol";
import {SubnetActorMock} from "../mocks/SubnetActorMock.sol";
import {SubnetActorGetterFacet} from "../../src/subnet/SubnetActorGetterFacet.sol";
import {IntegrationTestBase} from "../IntegrationTestBase.sol";
import {SupplySourceHelper} from "../../src/lib/SupplySourceHelper.sol";
import {GatewayFacetsHelper} from "../helpers/GatewayFacetsHelper.sol";
import {SubnetActorFacetsHelper} from "../helpers/SubnetActorFacetsHelper.sol";
import {GatewayFacetsHelper} from "../helpers/GatewayFacetsHelper.sol";

contract SubnetActorInvariants is StdInvariant, IntegrationTestBase {
    using SubnetIDHelper for SubnetID;
    using GatewayFacetsHelper for GatewayDiamond;
    using SubnetActorFacetsHelper for SubnetActorDiamond;

    SubnetActorHandler private subnetActorHandler;

    address gatewayAddress;

    function setUp() public override {
        GatewayDiamond.ConstructorParams memory gwConstructorParams = defaultGatewayParams();

        gatewayDiamond = createGatewayDiamond(gwConstructorParams);

        gatewayAddress = address(gatewayDiamond);

        saDiamond = createMockedSubnetActorWithGateway(gatewayAddress);

        saMock = SubnetActorMock(address(saDiamond));
        subnetActorHandler = new SubnetActorHandler(saDiamond);

        bytes4[] memory fuzzSelectors = new bytes4[](4);
        fuzzSelectors[0] = SubnetActorHandler.join.selector;
        fuzzSelectors[1] = SubnetActorHandler.leave.selector;
        fuzzSelectors[2] = SubnetActorHandler.stake.selector;
        fuzzSelectors[3] = SubnetActorHandler.unstake.selector;

        targetSelector(FuzzSelector({addr: address(subnetActorHandler), selectors: fuzzSelectors}));
        targetContract(address(subnetActorHandler));
    }

    /// @notice The number of validators called `join` is equal to the number of total validators,
    /// if confirmations are executed immediately.
    function invariant_SA_01_total_validators_number_is_correct() public {
        assertEq(
            saDiamond.getter().getTotalValidatorsNumber(),
            subnetActorHandler.joinedValidatorsNumber(),
            "unexpected total validators number"
        );
    }

    /// @notice The stake of the subnet is the same from the SubnetActor and SubnetActorHandler perspectives.
    /// @dev Confirmations are executed immediately via the mocked manager facet.
    /// forge-config: default.invariant.runs = 50
    /// forge-config: default.invariant.depth = 100
    /// forge-config: default.invariant.fail-on-revert = false
    function invariant_SA_02_conservationOfETH() public {
        assertEq(
            ETH_SUPPLY,
            address(subnetActorHandler).balance + subnetActorHandler.ghost_stakedSum(),
            "subnet actor handler: unexpected stake"
        );
        assertEq(
            ETH_SUPPLY,
            address(subnetActorHandler).balance +
                saDiamond.getter().getTotalCollateral() +
                subnetActorHandler.ghost_unstakedSum(),
            "subnet actor: unexpected stake"
        );
        assertEq(
            ETH_SUPPLY,
            address(subnetActorHandler).balance +
                saDiamond.getter().getTotalConfirmedCollateral() +
                subnetActorHandler.ghost_unstakedSum(),
            "subnet actor: unexpected stake"
        );

        if (saDiamond.getter().bootstrapped()) {
            SubnetID memory subnetId = gatewayDiamond.getter().getNetworkName().createSubnetId(address(saDiamond));
            Subnet memory subnet = gatewayDiamond.getter().subnets(subnetId.toHash());

            assertEq(
                subnetActorHandler.ghost_stakedSum() - subnetActorHandler.ghost_unstakedSum(),
                subnet.stake,
                "gateway actor: unexpected stake"
            );
        }
    }

    /// @notice The value resulting from all stake and unstake operations is equal to the total confirmed collateral.
    function invariant_SA_03_sum_of_stake_equals_collateral() public {
        assertEq(
            saDiamond.getter().getTotalConfirmedCollateral(),
            subnetActorHandler.ghost_stakedSum() - subnetActorHandler.ghost_unstakedSum()
        );
    }

    /// @notice Validator can withdraw all ETHs that it staked after leaving.
    /// forge-config: default.invariant.runs = 500
    /// forge-config: default.invariant.depth = 5
    function invariant_SA_04_validator_can_claim_collateral() public {
        address validator = subnetActorHandler.leave(0);
        if (validator == address(0)) {
            return;
        }
        if (!saDiamond.getter().bootstrapped()) {
            return;
        }

        uint256 subnetBalanceBefore = address(saDiamond).balance;
        uint256 balanceBefore = validator.balance;

        vm.prank(validator);
        saMock.claim();
        saMock.confirmNextChange();

        uint256 balanceAfter = validator.balance;
        uint256 subnetBalanceAfter = address(saDiamond).balance;

        assertEq(balanceAfter - balanceBefore, subnetBalanceBefore - subnetBalanceAfter, "unexpected claim amount");
    }

    /// @notice Total confirmed collateral equals sum of validator collaterals.
    function invariant_SA_05_total_collateral_equals_sum_of_validator_collaterals() public {
        uint256 sumOfCollaterals;
        address[] memory validators = subnetActorHandler.joinedValidators();
        uint256 n = validators.length;
        for (uint256 i; i < n; ++i) {
            sumOfCollaterals += saDiamond.getter().getTotalValidatorCollateral(validators[i]);
        }

        uint256 totalCollateral = saDiamond.getter().getTotalConfirmedCollateral();

        assertEq(sumOfCollaterals, totalCollateral, "unexpected sum of validators collateral");
    }
}
