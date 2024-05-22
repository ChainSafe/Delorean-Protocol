// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {StdInvariant, Test} from "forge-std/Test.sol";
import {TestUtils} from "../helpers/TestUtils.sol";
import {IDiamond} from "../../src/interfaces/IDiamond.sol";
import {SubnetRegistryHandler} from "./handlers/SubnetRegistryHandler.sol";
import {SubnetRegistryDiamond} from "../../src/SubnetRegistryDiamond.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";
import {SubnetActorGetterFacet} from "../../src/subnet/SubnetActorGetterFacet.sol";
import {SubnetActorManagerFacet} from "../../src/subnet/SubnetActorManagerFacet.sol";
import {SubnetActorPauseFacet} from "../../src/subnet/SubnetActorPauseFacet.sol";
import {SubnetActorCheckpointingFacet} from "../../src/subnet/SubnetActorCheckpointingFacet.sol";
import {SubnetActorRewardFacet} from "../../src/subnet/SubnetActorRewardFacet.sol";
import {SubnetID} from "../../src/structs/Subnet.sol";
import {RegisterSubnetFacet} from "../../src/subnetregistry/RegisterSubnetFacet.sol";
import {SubnetGetterFacet} from "../../src/subnetregistry/SubnetGetterFacet.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {OwnershipFacet} from "../../src/OwnershipFacet.sol";
import {IntegrationTestBase, TestRegistry} from "../IntegrationTestBase.sol";
import {SelectorLibrary} from "../helpers/SelectorLibrary.sol";

contract SubnetRegistryInvariants is StdInvariant, Test, TestRegistry, IntegrationTestBase {
    SubnetRegistryHandler private registryHandler;

    function setUp() public virtual override {
        bytes4[] memory mockedSelectors = new bytes4[](1);
        mockedSelectors[0] = 0x6cb2ecee;

        bytes4[] memory mockedSelectors2 = new bytes4[](1);
        mockedSelectors2[0] = 0x133f74ea;

        bytes4[] memory mockedSelectors3 = new bytes4[](1);
        mockedSelectors3[0] = 0x433f74ea;

        bytes4[] memory mockedSelectors4 = new bytes4[](1);
        mockedSelectors4[0] = 0x333f74ea;

        bytes4[] memory mockedSelectors5 = new bytes4[](1);
        mockedSelectors5[0] = 0x233f74ea;

        SubnetRegistryDiamond.ConstructorParams memory params;
        params.gateway = DEFAULT_IPC_GATEWAY_ADDR;

        params.getterFacet = address(new SubnetActorGetterFacet());
        params.managerFacet = address(new SubnetActorManagerFacet());
        params.rewarderFacet = address(new SubnetActorRewardFacet());
        params.checkpointerFacet = address(new SubnetActorCheckpointingFacet());
        params.pauserFacet = address(new SubnetActorPauseFacet());
        params.diamondCutFacet = address(new DiamondCutFacet());
        params.diamondLoupeFacet = address(new DiamondLoupeFacet());
        params.ownershipFacet = address(new OwnershipFacet());

        params.subnetActorGetterSelectors = mockedSelectors;
        params.subnetActorManagerSelectors = mockedSelectors2;
        params.subnetActorRewarderSelectors = mockedSelectors3;
        params.subnetActorCheckpointerSelectors = mockedSelectors4;
        params.subnetActorPauserSelectors = mockedSelectors5;
        params.subnetActorDiamondCutSelectors = SelectorLibrary.resolveSelectors("DiamondCutFacet");
        params.subnetActorDiamondLoupeSelectors = SelectorLibrary.resolveSelectors("DiamondLoupeFacet");
        params.subnetActorOwnershipSelectors = SelectorLibrary.resolveSelectors("OwnershipFacet");

        registryDiamond = createSubnetRegistry(params);
        registryHandler = new SubnetRegistryHandler(registryDiamond);

        bytes4[] memory fuzzSelectors = new bytes4[](1);
        fuzzSelectors[0] = SubnetRegistryHandler.deploySubnetActorFromRegistry.selector;

        targetSelector(FuzzSelector({addr: address(registryHandler), selectors: fuzzSelectors}));
        targetContract(address(registryHandler));
    }

    /// @notice The Gateway address is not changed.
    /// forge-config: default.invariant.runs = 5
    /// forge-config: default.invariant.depth = 10
    /// forge-config: default.invariant.fail-on-revert = false
    function invariant_SR_01_gateway_address_is_persistent() public {
        assertEq(registryHandler.getGateway(), DEFAULT_IPC_GATEWAY_ADDR);
    }

    /// @notice If a subnet was created then it's address can be retrieved.
    /// TODO: this test has the same issue as https://github.com/foundry-rs/foundry/issues/6074
    /// We may need to update the test setup when the issue is fixed.
    ///
    /// forge-config: default.invariant.runs = 50
    /// forge-config: default.invariant.depth = 10
    /// forge-config: default.invariant.fail-on-revert = false
    function invariant_SR_02_subnet_address_can_be_retrieved() public {
        address[] memory owners = registryHandler.getOwners();
        uint256 length = owners.length;
        if (length == 0) {
            return;
        }
        for (uint256 i; i < length; ++i) {
            address owner = owners[i];
            uint64 nonce = registryHandler.getUserLastNonce(owner);

            assertNotEq(nonce, 0);
            assertEq(
                registryHandler.getSubnetDeployedBy(owner),
                registryHandler.getSubnetDeployedWithNonce(owner, nonce)
            );
        }
    }
}
