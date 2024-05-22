// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "../../src/errors/IPCErrors.sol";
import "forge-std/Test.sol";

import {ConsensusType} from "../../src/enums/ConsensusType.sol";
import {TestUtils} from "../helpers/TestUtils.sol";
import {IERC165} from "../../src/interfaces/IERC165.sol";
import {IDiamond} from "../../src/interfaces/IDiamond.sol";
import {IDiamondCut} from "../../src/interfaces/IDiamondCut.sol";
import {IDiamondLoupe} from "../../src/interfaces/IDiamondLoupe.sol";
import {LibDiamond} from "../../src/lib/LibDiamond.sol";

import {SubnetActorGetterFacet} from "../../src/subnet/SubnetActorGetterFacet.sol";
import {SubnetActorManagerFacet} from "../../src/subnet/SubnetActorManagerFacet.sol";
import {SubnetActorPauseFacet} from "../../src/subnet/SubnetActorPauseFacet.sol";
import {SubnetActorCheckpointingFacet} from "../../src/subnet/SubnetActorCheckpointingFacet.sol";
import {SubnetActorRewardFacet} from "../../src/subnet/SubnetActorRewardFacet.sol";
import {SubnetActorDiamond} from "../../src/SubnetActorDiamond.sol";
import {SubnetID, PermissionMode, SubnetCreationPrivileges} from "../../src/structs/Subnet.sol";
import {SubnetRegistryDiamond} from "../../src/SubnetRegistryDiamond.sol";

import {RegisterSubnetFacet} from "../../src/subnetregistry/RegisterSubnetFacet.sol";
import {SubnetGetterFacet} from "../../src/subnetregistry/SubnetGetterFacet.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {OwnershipFacet} from "../../src/OwnershipFacet.sol";
import {SupplySourceHelper} from "../../src/lib/SupplySourceHelper.sol";
import {RegistryFacetsHelper} from "../helpers/RegistryFacetsHelper.sol";
import {DiamondFacetsHelper} from "../helpers/DiamondFacetsHelper.sol";

import {SelectorLibrary} from "../helpers/SelectorLibrary.sol";

import {IntegrationTestBase, TestRegistry} from "../IntegrationTestBase.sol";

contract SubnetRegistryTest is Test, TestRegistry, IntegrationTestBase {
    using RegistryFacetsHelper for SubnetRegistryDiamond;
    using DiamondFacetsHelper for SubnetRegistryDiamond;

    bytes4[] empty;

    function defaultParams() internal returns (SubnetRegistryDiamond.ConstructorParams memory params) {
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

        params.creationPrivileges = SubnetCreationPrivileges.Unrestricted;

        return params;
    }

    function setUp() public virtual override {
        SubnetRegistryDiamond.ConstructorParams memory params = defaultParams();

        registryDiamond = createSubnetRegistry(params);
        registryLouper = registryDiamond.diamondLouper();
        registryCutter = registryDiamond.diamondCutter();
        registrySubnetFacet = registryDiamond.register();
        registrySubnetGetterFacet = registryDiamond.getter();
    }

    function test_Registry_NoPermission() public {
        SubnetRegistryDiamond.ConstructorParams memory p = defaultParams();
        p.creationPrivileges = SubnetCreationPrivileges.Owner;

        SubnetRegistryDiamond s = createSubnetRegistry(p);

        SubnetActorDiamond.ConstructorParams memory params = defaultSubnetActorParamsWith(DEFAULT_IPC_GATEWAY_ADDR);
        params.permissionMode = PermissionMode.Collateral;

        vm.prank(address(1));
        vm.expectRevert(LibDiamond.NotOwner.selector);
        s.register().newSubnetActor(params);
    }

    function test_Registry_FacetFunctionSelectors() public view {
        IDiamondLoupe.Facet[] memory facets;
        uint256 facetsLength = facets.length;
        for (uint256 i = 0; i < facetsLength; ++i) {
            address facetAddress = facets[i].facetAddress;
            require(
                registryLouper.facetFunctionSelectors(facetAddress).length == facets[i].functionSelectors.length,
                "unexpected function selector length"
            );
        }
    }

    function test_Registry_Deployment_IERC165() public view {
        require(registryLouper.facets().length == 5, "unexpected length");
        require(registryLouper.facetAddresses().length == registryLouper.facets().length, "inconsistent diamond size");
        require(registryLouper.supportsInterface(type(IERC165).interfaceId) == true, "IERC165 not supported");
        require(registryLouper.supportsInterface(type(IDiamondCut).interfaceId) == true, "IDiamondCut not supported");
        require(
            registryLouper.supportsInterface(type(IDiamondLoupe).interfaceId) == true,
            "IDiamondLoupe not supported"
        );
    }

    function test_Registry_Deployment_ZeroAddressFacet() public {
        SubnetRegistryDiamond.ConstructorParams memory params;
        params.gateway = DEFAULT_IPC_GATEWAY_ADDR;
        params.subnetActorGetterSelectors = empty;
        params.subnetActorManagerSelectors = empty;
        params.subnetActorDiamondLoupeSelectors = empty;
        params.subnetActorDiamondCutSelectors = empty;
        params.subnetActorOwnershipSelectors = empty;

        IDiamond.FacetCut[] memory diamondCut = new IDiamond.FacetCut[](0);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.getterFacet = address(1);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.managerFacet = address(2);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.rewarderFacet = address(3);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.checkpointerFacet = address(4);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.pauserFacet = address(5);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.diamondLoupeFacet = address(6);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.diamondCutFacet = address(7);
        vm.expectRevert(FacetCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);

        params.ownershipFacet = address(8);
        new SubnetRegistryDiamond(diamondCut, params);
    }

    function test_Registry_Deployment_ZeroGateway() public {
        SubnetRegistryDiamond.ConstructorParams memory params;
        params.gateway = address(0);
        params.getterFacet = address(1);
        params.managerFacet = address(1);
        params.subnetActorGetterSelectors = empty;
        params.subnetActorManagerSelectors = empty;

        IDiamond.FacetCut[] memory diamondCut = new IDiamond.FacetCut[](0);
        vm.expectRevert(GatewayCannotBeZero.selector);
        new SubnetRegistryDiamond(diamondCut, params);
    }

    function test_Registry_Deployment_DifferentGateway() public {
        SubnetActorDiamond.ConstructorParams memory params = defaultSubnetActorParamsWith(address(1));
        params.permissionMode = PermissionMode.Collateral;

        vm.expectRevert(WrongGateway.selector);
        registrySubnetFacet.newSubnetActor(params);
    }

    function test_Registry_LatestSubnetDeploy_Revert() public {
        vm.startPrank(DEFAULT_SENDER);

        SubnetActorDiamond.ConstructorParams memory params = defaultSubnetActorParamsWith(DEFAULT_IPC_GATEWAY_ADDR);
        params.permissionMode = PermissionMode.Collateral;

        registrySubnetFacet.newSubnetActor(params);
        vm.expectRevert(CannotFindSubnet.selector);
        registrySubnetGetterFacet.latestSubnetDeployed(address(0));
    }

    function test_Registry_GetSubnetDeployedByNonce_Revert() public {
        vm.startPrank(DEFAULT_SENDER);

        SubnetActorDiamond.ConstructorParams memory params = defaultSubnetActorParamsWith(DEFAULT_IPC_GATEWAY_ADDR);
        params.permissionMode = PermissionMode.Collateral;

        registrySubnetFacet.newSubnetActor(params);
        vm.expectRevert(CannotFindSubnet.selector);
        registrySubnetGetterFacet.getSubnetDeployedByNonce(address(0), 1);
    }

    function test_Registry_Deployment_Works() public {
        vm.startPrank(DEFAULT_SENDER);

        SubnetActorDiamond.ConstructorParams memory params = defaultSubnetActorParamsWith(DEFAULT_IPC_GATEWAY_ADDR);
        registrySubnetFacet.newSubnetActor(params);
        require(registrySubnetGetterFacet.latestSubnetDeployed(DEFAULT_SENDER) != address(0));
    }

    function test_deploySubnetActor_fuzz(
        uint256 _minCollateral,
        uint64 _minValidators,
        uint64 _bottomUpCheckPeriod,
        uint16 _activeValidatorsLimit,
        uint8 _majorityPercentage,
        uint8 _pathSize,
        int8 _powerScale
    ) public {
        vm.assume(_minCollateral > 0);
        vm.assume(_bottomUpCheckPeriod > 0);
        vm.assume(_majorityPercentage >= 51 && _majorityPercentage <= 100);
        vm.assume(_powerScale <= 18);
        vm.assume(_pathSize >= 0 && _pathSize <= 5);

        address[] memory path = new address[](_pathSize);
        for (uint8 i; i < _pathSize; ++i) {
            path[i] = vm.addr(300 + i);
        }

        SubnetActorDiamond.ConstructorParams memory params = SubnetActorDiamond.ConstructorParams({
            parentId: SubnetID({root: ROOTNET_CHAINID, route: path}),
            ipcGatewayAddr: DEFAULT_IPC_GATEWAY_ADDR,
            consensus: ConsensusType.Fendermint,
            minActivationCollateral: _minCollateral,
            minValidators: _minValidators,
            bottomUpCheckPeriod: _bottomUpCheckPeriod,
            majorityPercentage: _majorityPercentage,
            activeValidatorsLimit: _activeValidatorsLimit,
            powerScale: _powerScale,
            permissionMode: PermissionMode.Collateral,
            supplySource: SupplySourceHelper.native()
        });

        registrySubnetFacet.newSubnetActor(params);
    }

    // Test the updateReferenceSubnetContract method
    function test_UpdateReferenceSubnetContract() public {
        // Prepare new facet addresses and selector arrays
        address newGetterFacet = address(2); // Mocked new facet address
        address newManagerFacet = address(3); // Mocked new facet address
        bytes4[] memory newSubnetGetterSelectors = new bytes4[](1);
        newSubnetGetterSelectors[0] = 0x12345678; // Mocked selector
        bytes4[] memory newSubnetManagerSelectors = new bytes4[](1);
        newSubnetManagerSelectors[0] = 0x87654322; // Mocked selector

        registrySubnetGetterFacet.updateReferenceSubnetContract(
            newGetterFacet,
            newManagerFacet,
            newSubnetGetterSelectors,
            newSubnetManagerSelectors
        );

        // Validate the updates
        require(
            address(registrySubnetGetterFacet.getSubnetActorGetterFacet()) == newGetterFacet,
            "Getter facet address not updated correctly"
        );
        require(
            address(registrySubnetGetterFacet.getSubnetActorManagerFacet()) == newManagerFacet,
            "Manager facet address not updated correctly"
        );

        // Validate the updates for subnetGetterSelectors
        bytes4[] memory currentSubnetGetterSelectors = registrySubnetGetterFacet.getSubnetActorGetterSelectors();
        TestUtils.validateBytes4Array(
            currentSubnetGetterSelectors,
            newSubnetGetterSelectors,
            "SubnetGetterSelectors mismatch"
        );

        // Validate the updates for subnetManagerSelectors
        bytes4[] memory currentSubnetManagerSelectors = registrySubnetGetterFacet.getSubnetActorManagerSelectors();
        TestUtils.validateBytes4Array(
            currentSubnetManagerSelectors,
            newSubnetManagerSelectors,
            "SubnetManagerSelectors mismatch"
        );

        // Test only owner can update
        vm.prank(address(1)); // Set a different address as the sender
        vm.expectRevert(abi.encodeWithSelector(LibDiamond.NotOwner.selector)); // Expected revert message
        registrySubnetGetterFacet.updateReferenceSubnetContract(
            newGetterFacet,
            newManagerFacet,
            newSubnetGetterSelectors,
            newSubnetManagerSelectors
        );
    }
}
