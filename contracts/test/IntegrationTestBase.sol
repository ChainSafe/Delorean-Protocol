// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "../src/errors/IPCErrors.sol";

import {EMPTY_BYTES, METHOD_SEND} from "../src/constants/Constants.sol";
import {ConsensusType} from "../src/enums/ConsensusType.sol";
import {IDiamond} from "../src/interfaces/IDiamond.sol";
import {IpcEnvelope, BottomUpCheckpoint, IpcMsgKind, ParentFinality, CallMsg} from "../src/structs/CrossNet.sol";
import {FvmAddress} from "../src/structs/FvmAddress.sol";
import {SubnetID, SupplyKind, PermissionMode, PermissionMode, Subnet, SupplySource, IPCAddress, Validator} from "../src/structs/Subnet.sol";
import {SubnetIDHelper} from "../src/lib/SubnetIDHelper.sol";
import {FvmAddressHelper} from "../src/lib/FvmAddressHelper.sol";
import {CrossMsgHelper} from "../src/lib/CrossMsgHelper.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {GatewayDiamond} from "../src/GatewayDiamond.sol";
import {SubnetActorDiamond} from "../src/SubnetActorDiamond.sol";
import {GatewayGetterFacet} from "../src/gateway/GatewayGetterFacet.sol";
import {GatewayMessengerFacet} from "../src/gateway/GatewayMessengerFacet.sol";
import {GatewayManagerFacet} from "../src/gateway/GatewayManagerFacet.sol";

import {CheckpointingFacet} from "../src/gateway/router/CheckpointingFacet.sol";
import {XnetMessagingFacet} from "../src/gateway/router/XnetMessagingFacet.sol";
import {TopDownFinalityFacet} from "../src/gateway/router/TopDownFinalityFacet.sol";

import {SubnetActorMock} from "./mocks/SubnetActorMock.sol";
import {SubnetActorManagerFacet} from "../src/subnet/SubnetActorManagerFacet.sol";
import {SubnetActorPauseFacet} from "../src/subnet/SubnetActorPauseFacet.sol";
import {SubnetActorCheckpointingFacet} from "../src/subnet/SubnetActorCheckpointingFacet.sol";
import {SubnetActorRewardFacet} from "../src/subnet/SubnetActorRewardFacet.sol";
import {SubnetActorGetterFacet} from "../src/subnet/SubnetActorGetterFacet.sol";

import {SubnetRegistryDiamond} from "../src/SubnetRegistryDiamond.sol";
import {RegisterSubnetFacet} from "../src/subnetregistry/RegisterSubnetFacet.sol";
import {SubnetGetterFacet} from "../src/subnetregistry/SubnetGetterFacet.sol";

import {OwnershipFacet} from "../src/OwnershipFacet.sol";

import {DiamondLoupeFacet} from "../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../src/diamond/DiamondCutFacet.sol";
import {SupplySourceHelper} from "../src/lib/SupplySourceHelper.sol";
import {TestUtils} from "./helpers/TestUtils.sol";
import {SelectorLibrary} from "./helpers/SelectorLibrary.sol";
import {GatewayFacetsHelper} from "./helpers/GatewayFacetsHelper.sol";
import {SubnetActorFacetsHelper} from "./helpers/SubnetActorFacetsHelper.sol";
import {DiamondFacetsHelper} from "./helpers/DiamondFacetsHelper.sol";

struct TestSubnetDefinition {
    GatewayDiamond gateway;
    address gatewayAddr;
    SubnetActorDiamond subnetActor;
    address subnetActorAddr;
    SubnetID id;
    address[] path;
}

struct RootSubnetDefinition {
    GatewayDiamond gateway;
    address gatewayAddr;
    SubnetID id;
}

contract TestParams {
    uint64 constant MAX_NONCE = type(uint64).max;
    address constant BLS_ACCOUNT_ADDREESS = address(0xfF000000000000000000000000000000bEefbEEf);
    uint64 constant DEFAULT_MIN_VALIDATORS = 1;
    uint256 constant DEFAULT_MIN_VALIDATOR_STAKE = 1 ether;
    uint8 constant DEFAULT_MAJORITY_PERCENTAGE = 70;
    uint64 constant DEFAULT_COLLATERAL_AMOUNT = 1 ether;
    uint64 constant DEFAULT_CHECKPOINT_PERIOD = 10;
    string constant DEFAULT_NET_ADDR = "netAddr";
    bytes constant GENESIS = EMPTY_BYTES;
    uint256 constant DEFAULT_CROSS_MSG_FEE = 10 gwei;
    uint256 constant DEFAULT_RELAYER_REWARD = 10 gwei;
    address constant CHILD_NETWORK_ADDRESS = address(10);
    address constant CHILD_NETWORK_ADDRESS_2 = address(11);
    uint64 constant EPOCH_ONE = 1 * DEFAULT_CHECKPOINT_PERIOD;
    uint256 constant INITIAL_VALIDATOR_FUNDS = 1 ether;
    uint16 constant DEFAULT_ACTIVE_VALIDATORS_LIMIT = 100;
    int8 constant DEFAULT_POWER_SCALE = 12;
    uint64 constant ROOTNET_CHAINID = 123;
    address constant ROOTNET_ADDRESS = address(1);
    address constant DEFAULT_IPC_GATEWAY_ADDR = address(1024);
    address constant TOPDOWN_VALIDATOR_1 = address(12);
    bytes32 constant DEFAULT_COMMIT_SHA = "c7d8f53f";
}

contract TestRegistry is Test, TestParams {
    bytes4[] registerSubnetFacetSelectors;
    bytes4[] registerSubnetGetterFacetSelectors;
    bytes4[] registerCutterSelectors;
    bytes4[] registerLouperSelectors;
    bytes4[] registerOwnershipSelectors;

    SubnetRegistryDiamond registryDiamond;
    DiamondLoupeFacet registryLouper;
    DiamondCutFacet registryCutter;
    RegisterSubnetFacet registrySubnetFacet;
    SubnetGetterFacet registrySubnetGetterFacet;
    OwnershipFacet ownershipFacet;

    constructor() {
        registerSubnetFacetSelectors = SelectorLibrary.resolveSelectors("RegisterSubnetFacet");
        registerSubnetGetterFacetSelectors = SelectorLibrary.resolveSelectors("SubnetGetterFacet");
        registerCutterSelectors = SelectorLibrary.resolveSelectors("DiamondCutFacet");
        registerLouperSelectors = SelectorLibrary.resolveSelectors("DiamondLoupeFacet");
        registerOwnershipSelectors = SelectorLibrary.resolveSelectors("OwnershipFacet");
    }
}

contract TestGatewayActor is Test, TestParams {
    bytes4[] gwCheckpointingFacetSelectors;
    bytes4[] gwXnetMessagingFacetSelectors;
    bytes4[] gwTopDownFinalityFacetSelectors;

    bytes4[] gwManagerSelectors;
    bytes4[] gwGetterSelectors;
    bytes4[] gwMessengerSelectors;

    bytes4[] gwCutterSelectors;
    bytes4[] gwLoupeSelectors;

    bytes4[] gwOwnershipSelectors;

    GatewayDiamond gatewayDiamond;

    constructor() {
        gwCheckpointingFacetSelectors = SelectorLibrary.resolveSelectors("CheckpointingFacet");
        gwXnetMessagingFacetSelectors = SelectorLibrary.resolveSelectors("XnetMessagingFacet");
        gwTopDownFinalityFacetSelectors = SelectorLibrary.resolveSelectors("TopDownFinalityFacet");

        gwGetterSelectors = SelectorLibrary.resolveSelectors("GatewayGetterFacet");
        gwManagerSelectors = SelectorLibrary.resolveSelectors("GatewayManagerFacet");
        gwMessengerSelectors = SelectorLibrary.resolveSelectors("GatewayMessengerFacet");
        gwCutterSelectors = SelectorLibrary.resolveSelectors("DiamondCutFacet");
        gwLoupeSelectors = SelectorLibrary.resolveSelectors("DiamondLoupeFacet");

        gwOwnershipSelectors = SelectorLibrary.resolveSelectors("OwnershipFacet");
    }
}

contract TestSubnetActor is Test, TestParams {
    bytes4[] saGetterSelectors;
    bytes4[] saManagerSelectors;
    bytes4[] saPauserSelectors;
    bytes4[] saRewarderSelectors;
    bytes4[] saCheckpointerSelectors;
    bytes4[] saManagerMockedSelectors;
    bytes4[] saCutterSelectors;
    bytes4[] saLouperSelectors;
    bytes4[] saOwnershipSelectors;

    SubnetActorDiamond saDiamond;
    SubnetActorMock saMock;

    constructor() {
        saGetterSelectors = SelectorLibrary.resolveSelectors("SubnetActorGetterFacet");
        saManagerSelectors = SelectorLibrary.resolveSelectors("SubnetActorManagerFacet");
        saPauserSelectors = SelectorLibrary.resolveSelectors("SubnetActorPauseFacet");
        saRewarderSelectors = SelectorLibrary.resolveSelectors("SubnetActorRewardFacet");
        saCheckpointerSelectors = SelectorLibrary.resolveSelectors("SubnetActorCheckpointingFacet");
        saManagerMockedSelectors = SelectorLibrary.resolveSelectors("SubnetActorMock");
        saCutterSelectors = SelectorLibrary.resolveSelectors("DiamondCutFacet");
        saLouperSelectors = SelectorLibrary.resolveSelectors("DiamondLoupeFacet");
        saOwnershipSelectors = SelectorLibrary.resolveSelectors("OwnershipFacet");
    }

    function defaultSubnetActorParamsWith(
        address gw,
        SubnetID memory parentID
    ) internal pure returns (SubnetActorDiamond.ConstructorParams memory) {
        SupplySource memory native = SupplySourceHelper.native();
        return defaultSubnetActorParamsWith(gw, parentID, native);
    }

    function defaultSubnetActorParamsWith(
        address gw,
        SubnetID memory parentID,
        SupplySource memory source
    ) internal pure returns (SubnetActorDiamond.ConstructorParams memory) {
        SubnetActorDiamond.ConstructorParams memory params = SubnetActorDiamond.ConstructorParams({
            parentId: parentID,
            ipcGatewayAddr: gw,
            consensus: ConsensusType.Fendermint,
            minActivationCollateral: DEFAULT_COLLATERAL_AMOUNT,
            minValidators: DEFAULT_MIN_VALIDATORS,
            bottomUpCheckPeriod: DEFAULT_CHECKPOINT_PERIOD,
            majorityPercentage: DEFAULT_MAJORITY_PERCENTAGE,
            activeValidatorsLimit: DEFAULT_ACTIVE_VALIDATORS_LIMIT,
            powerScale: DEFAULT_POWER_SCALE,
            permissionMode: PermissionMode.Collateral,
            supplySource: source
        });
        return params;
    }

    function defaultSubnetActorParamsWith(
        address gw
    ) internal pure virtual returns (SubnetActorDiamond.ConstructorParams memory) {
        return
            defaultSubnetActorParamsWith(
                gw,
                SubnetID({root: ROOTNET_CHAINID, route: new address[](0)}),
                SupplySourceHelper.native()
            );
    }

    function defaultSubnetActorParamsWith(
        address gw,
        SubnetID memory parentID,
        address tokenAddress
    ) internal pure returns (SubnetActorDiamond.ConstructorParams memory) {
        SubnetActorDiamond.ConstructorParams memory params = SubnetActorDiamond.ConstructorParams({
            parentId: parentID,
            ipcGatewayAddr: gw,
            consensus: ConsensusType.Fendermint,
            minActivationCollateral: DEFAULT_COLLATERAL_AMOUNT,
            minValidators: DEFAULT_MIN_VALIDATORS,
            bottomUpCheckPeriod: DEFAULT_CHECKPOINT_PERIOD,
            majorityPercentage: DEFAULT_MAJORITY_PERCENTAGE,
            activeValidatorsLimit: DEFAULT_ACTIVE_VALIDATORS_LIMIT,
            powerScale: DEFAULT_POWER_SCALE,
            permissionMode: PermissionMode.Collateral,
            supplySource: SupplySource({kind: SupplyKind.ERC20, tokenAddress: tokenAddress})
        });
        return params;
    }
}

contract IntegrationTestBase is Test, TestParams, TestRegistry, TestSubnetActor, TestGatewayActor {
    using SubnetIDHelper for SubnetID;
    using SupplySourceHelper for SupplySource;
    using CrossMsgHelper for IpcEnvelope;
    using FvmAddressHelper for FvmAddress;
    using GatewayFacetsHelper for address;
    using GatewayFacetsHelper for GatewayDiamond;
    using SubnetActorFacetsHelper for address;
    using SubnetActorFacetsHelper for SubnetActorDiamond;
    using DiamondFacetsHelper for address;
    using DiamondFacetsHelper for GatewayDiamond;
    using DiamondFacetsHelper for SubnetActorDiamond;

    event SubnetRegistryCreated(address indexed subnetRegistryAddress);

    constructor() {}

    function setUp() public virtual {
        address[] memory path = new address[](1);
        path[0] = ROOTNET_ADDRESS;

        // create the root gateway actor.
        GatewayDiamond.ConstructorParams memory gwConstructorParams = defaultGatewayParams();
        gatewayDiamond = createGatewayDiamond(gwConstructorParams);

        // create a subnet actor in the root network.
        SubnetActorDiamond.ConstructorParams memory saConstructorParams = defaultSubnetActorParamsWith(
            address(gatewayDiamond)
        );

        saDiamond = createSubnetActor(saConstructorParams);

        addValidator(TOPDOWN_VALIDATOR_1, 100);
    }

    function defaultGatewayParams() internal pure virtual returns (GatewayDiamond.ConstructorParams memory) {
        GatewayDiamond.ConstructorParams memory params = GatewayDiamond.ConstructorParams({
            networkName: SubnetID({root: ROOTNET_CHAINID, route: new address[](0)}),
            bottomUpCheckPeriod: DEFAULT_CHECKPOINT_PERIOD,
            majorityPercentage: DEFAULT_MAJORITY_PERCENTAGE,
            genesisValidators: new Validator[](0),
            activeValidatorsLimit: DEFAULT_ACTIVE_VALIDATORS_LIMIT,
            commitSha: DEFAULT_COMMIT_SHA
        });
        return params;
    }

    function gatewayParams(SubnetID memory id) internal pure returns (GatewayDiamond.ConstructorParams memory) {
        GatewayDiamond.ConstructorParams memory params = GatewayDiamond.ConstructorParams({
            networkName: id,
            bottomUpCheckPeriod: DEFAULT_CHECKPOINT_PERIOD,
            majorityPercentage: DEFAULT_MAJORITY_PERCENTAGE,
            genesisValidators: new Validator[](0),
            activeValidatorsLimit: DEFAULT_ACTIVE_VALIDATORS_LIMIT,
            commitSha: DEFAULT_COMMIT_SHA
        });
        return params;
    }

    function createGatewayDiamond(GatewayDiamond.ConstructorParams memory params) public returns (GatewayDiamond) {
        CheckpointingFacet checkpointingFacet = new CheckpointingFacet();
        XnetMessagingFacet xnetMessagingFacet = new XnetMessagingFacet();
        TopDownFinalityFacet topDownFinalityFacet = new TopDownFinalityFacet();
        GatewayManagerFacet manager = new GatewayManagerFacet();
        GatewayGetterFacet getter = new GatewayGetterFacet();
        GatewayMessengerFacet messenger = new GatewayMessengerFacet();
        DiamondCutFacet cutter = new DiamondCutFacet();
        DiamondLoupeFacet louper = new DiamondLoupeFacet();
        OwnershipFacet ownership = new OwnershipFacet();

        IDiamond.FacetCut[] memory gwDiamondCut = new IDiamond.FacetCut[](9);

        gwDiamondCut[0] = (
            IDiamond.FacetCut({
                facetAddress: address(checkpointingFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwCheckpointingFacetSelectors
            })
        );

        gwDiamondCut[6] = (
            IDiamond.FacetCut({
                facetAddress: address(xnetMessagingFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwXnetMessagingFacetSelectors
            })
        );

        gwDiamondCut[7] = (
            IDiamond.FacetCut({
                facetAddress: address(topDownFinalityFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwTopDownFinalityFacetSelectors
            })
        );

        gwDiamondCut[1] = (
            IDiamond.FacetCut({
                facetAddress: address(manager),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwManagerSelectors
            })
        );

        gwDiamondCut[2] = (
            IDiamond.FacetCut({
                facetAddress: address(getter),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwGetterSelectors
            })
        );

        gwDiamondCut[3] = (
            IDiamond.FacetCut({
                facetAddress: address(messenger),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwMessengerSelectors
            })
        );

        gwDiamondCut[4] = (
            IDiamond.FacetCut({
                facetAddress: address(louper),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwLoupeSelectors
            })
        );

        gwDiamondCut[5] = (
            IDiamond.FacetCut({
                facetAddress: address(cutter),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwCutterSelectors
            })
        );

        gwDiamondCut[8] = (
            IDiamond.FacetCut({
                facetAddress: address(ownership),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: gwOwnershipSelectors
            })
        );
        gatewayDiamond = new GatewayDiamond(gwDiamondCut, params);

        return gatewayDiamond;
    }

    function createSubnetActorDiamondWithFaucets(
        SubnetActorDiamond.ConstructorParams memory params,
        address getter,
        address manager,
        address pauser,
        address rewarder,
        address checkpointer,
        address ownership
    ) public returns (SubnetActorDiamond) {
        IDiamond.FacetCut[] memory diamondCut = new IDiamond.FacetCut[](6);

        diamondCut[0] = (
            IDiamond.FacetCut({
                facetAddress: getter,
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saGetterSelectors
            })
        );

        diamondCut[1] = (
            IDiamond.FacetCut({
                facetAddress: manager,
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saManagerSelectors
            })
        );

        diamondCut[2] = (
            IDiamond.FacetCut({
                facetAddress: pauser,
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saPauserSelectors
            })
        );

        diamondCut[3] = (
            IDiamond.FacetCut({
                facetAddress: rewarder,
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saRewarderSelectors
            })
        );

        diamondCut[4] = (
            IDiamond.FacetCut({
                facetAddress: checkpointer,
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saCheckpointerSelectors
            })
        );

        diamondCut[5] = (
            IDiamond.FacetCut({
                facetAddress: ownership,
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saOwnershipSelectors
            })
        );

        saDiamond = new SubnetActorDiamond(diamondCut, params, address(this));
        return saDiamond;
    }

    function createSubnetActor(SubnetActorDiamond.ConstructorParams memory params) public returns (SubnetActorDiamond) {
        SubnetActorManagerFacet manager = new SubnetActorManagerFacet();
        SubnetActorGetterFacet getter = new SubnetActorGetterFacet();
        SubnetActorPauseFacet pauser = new SubnetActorPauseFacet();
        SubnetActorRewardFacet rewarder = new SubnetActorRewardFacet();
        SubnetActorCheckpointingFacet checkpointer = new SubnetActorCheckpointingFacet();
        DiamondLoupeFacet louper = new DiamondLoupeFacet();
        DiamondCutFacet cutter = new DiamondCutFacet();
        OwnershipFacet ownership = new OwnershipFacet();

        IDiamond.FacetCut[] memory diamondCut = new IDiamond.FacetCut[](8);

        diamondCut[0] = (
            IDiamond.FacetCut({
                facetAddress: address(manager),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saManagerSelectors
            })
        );

        diamondCut[1] = (
            IDiamond.FacetCut({
                facetAddress: address(getter),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saGetterSelectors
            })
        );

        diamondCut[2] = (
            IDiamond.FacetCut({
                facetAddress: address(pauser),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saPauserSelectors
            })
        );

        diamondCut[3] = (
            IDiamond.FacetCut({
                facetAddress: address(rewarder),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saRewarderSelectors
            })
        );

        diamondCut[4] = (
            IDiamond.FacetCut({
                facetAddress: address(checkpointer),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saCheckpointerSelectors
            })
        );

        diamondCut[5] = (
            IDiamond.FacetCut({
                facetAddress: address(cutter),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saCutterSelectors
            })
        );

        diamondCut[6] = (
            IDiamond.FacetCut({
                facetAddress: address(louper),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saLouperSelectors
            })
        );

        diamondCut[7] = (
            IDiamond.FacetCut({
                facetAddress: address(ownership),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saOwnershipSelectors
            })
        );

        SubnetActorDiamond diamond = new SubnetActorDiamond(diamondCut, params, address(this));

        return diamond;
    }

    function createSubnetActor(
        address _ipcGatewayAddr,
        ConsensusType _consensus,
        uint256 _minActivationCollateral,
        uint64 _minValidators,
        uint64 _checkPeriod,
        uint8 _majorityPercentage
    ) public {
        createSubnetActor(
            _ipcGatewayAddr,
            _consensus,
            _minActivationCollateral,
            _minValidators,
            _checkPeriod,
            _majorityPercentage,
            PermissionMode.Collateral,
            100
        );
    }

    function createSubnetActor(
        address _ipcGatewayAddr,
        ConsensusType _consensus,
        uint256 _minActivationCollateral,
        uint64 _minValidators,
        uint64 _checkPeriod,
        uint8 _majorityPercentage,
        PermissionMode _permissionMode,
        uint16 _activeValidatorsLimit
    ) public {
        SubnetID memory _parentId = SubnetID(ROOTNET_CHAINID, new address[](0));

        SubnetActorDiamond.ConstructorParams memory params = SubnetActorDiamond.ConstructorParams({
            parentId: _parentId,
            ipcGatewayAddr: _ipcGatewayAddr,
            consensus: _consensus,
            minActivationCollateral: _minActivationCollateral,
            minValidators: _minValidators,
            bottomUpCheckPeriod: _checkPeriod,
            majorityPercentage: _majorityPercentage,
            activeValidatorsLimit: _activeValidatorsLimit,
            powerScale: 12,
            permissionMode: _permissionMode,
            supplySource: SupplySourceHelper.native()
        });
        saDiamond = createSubnetActor(params);
    }

    function createMockedSubnetActorWithGateway(address gw) public returns (SubnetActorDiamond) {
        SubnetActorMock mockedManager = new SubnetActorMock();
        SubnetActorGetterFacet getter = new SubnetActorGetterFacet();
        OwnershipFacet ownership = new OwnershipFacet();

        IDiamond.FacetCut[] memory diamondCut = new IDiamond.FacetCut[](3);

        diamondCut[0] = (
            IDiamond.FacetCut({
                facetAddress: address(mockedManager),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saManagerMockedSelectors
            })
        );

        diamondCut[1] = (
            IDiamond.FacetCut({
                facetAddress: address(getter),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saGetterSelectors
            })
        );

        diamondCut[2] = (
            IDiamond.FacetCut({
                facetAddress: address(ownership),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: saOwnershipSelectors
            })
        );

        SubnetActorDiamond.ConstructorParams memory params = defaultSubnetActorParamsWith(gw);

        SubnetActorDiamond d = new SubnetActorDiamond(diamondCut, params, address(this));

        return d;
    }

    // Creates a new SubnetRegistry contract.
    function createSubnetRegistry(
        SubnetRegistryDiamond.ConstructorParams memory params
    ) public returns (SubnetRegistryDiamond) {
        IDiamond.FacetCut[] memory diamondCut = new IDiamond.FacetCut[](5);

        DiamondCutFacet regCutFacet = new DiamondCutFacet();
        DiamondLoupeFacet regLoupeFacet = new DiamondLoupeFacet();
        RegisterSubnetFacet regSubnetFacet = new RegisterSubnetFacet();
        SubnetGetterFacet regGetterFacet = new SubnetGetterFacet();
        OwnershipFacet ownership = new OwnershipFacet();

        diamondCut[0] = (
            IDiamond.FacetCut({
                facetAddress: address(regLoupeFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: registerLouperSelectors
            })
        );
        diamondCut[1] = (
            IDiamond.FacetCut({
                facetAddress: address(regCutFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: registerCutterSelectors
            })
        );
        diamondCut[2] = (
            IDiamond.FacetCut({
                facetAddress: address(regSubnetFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: registerSubnetFacetSelectors
            })
        );
        diamondCut[3] = (
            IDiamond.FacetCut({
                facetAddress: address(regGetterFacet),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: registerSubnetGetterFacetSelectors
            })
        );

        diamondCut[4] = (
            IDiamond.FacetCut({
                facetAddress: address(ownership),
                action: IDiamond.FacetCutAction.Add,
                functionSelectors: registerOwnershipSelectors
            })
        );

        SubnetRegistryDiamond newSubnetRegistry = new SubnetRegistryDiamond(diamondCut, params);
        emit SubnetRegistryCreated(address(newSubnetRegistry));
        return newSubnetRegistry;
    }

    function totalWeight(uint256[] memory weights) public pure returns (uint256 sum) {
        for (uint64 i = 0; i < 3; i++) {
            sum += weights[i];
        }
        return sum;
    }

    function setupValidators() public returns (FvmAddress[] memory validators, address[] memory addresses) {
        validators = new FvmAddress[](3);
        validators[0] = FvmAddressHelper.from(vm.addr(100));
        validators[1] = FvmAddressHelper.from(vm.addr(200));
        validators[2] = FvmAddressHelper.from(vm.addr(300));

        addresses = new address[](3);
        addresses[0] = vm.addr(100);
        addresses[1] = vm.addr(200);
        addresses[2] = vm.addr(300);

        uint256[] memory weights = new uint256[](3);

        vm.deal(vm.addr(100), 1);
        vm.deal(vm.addr(200), 1);
        vm.deal(vm.addr(300), 1);

        weights[0] = 100;
        weights[1] = 100;
        weights[2] = 100;

        ParentFinality memory finality = ParentFinality({height: block.number, blockHash: bytes32(0)});

        vm.prank(FilAddress.SYSTEM_ACTOR);
        gatewayDiamond.topDownFinalizer().commitParentFinality(finality);
    }

    function setupWhiteListMethod(address caller, address src) public returns (bytes32) {
        registerSubnet(DEFAULT_COLLATERAL_AMOUNT, src);

        IpcEnvelope memory crossMsg = IpcEnvelope({
            kind: IpcMsgKind.Transfer,
            from: IPCAddress({
                subnetId: gatewayDiamond.getter().getNetworkName().createSubnetId(caller),
                rawAddress: FvmAddressHelper.from(caller)
            }),
            to: IPCAddress({
                subnetId: gatewayDiamond.getter().getNetworkName().createSubnetId(src),
                rawAddress: FvmAddressHelper.from(src)
            }),
            value: DEFAULT_CROSS_MSG_FEE + 1,
            nonce: 0,
            message: EMPTY_BYTES
        });
        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = crossMsg;

        // we add a validator with 10 times as much weight as the default validator.
        // This way we have 10/11 votes and we reach majority, setting the message in postbox
        // addValidator(caller, 1000);

        vm.prank(FilAddress.SYSTEM_ACTOR);
        gatewayDiamond.xnetMessenger().applyCrossMessages(msgs);

        return crossMsg.toHash();
    }

    function addValidator(address validator) public {
        addValidator(validator, 100);
    }

    function addValidator(address validator, uint256 weight) public {
        FvmAddress[] memory validators = new FvmAddress[](1);
        validators[0] = FvmAddressHelper.from(validator);
        uint256[] memory weights = new uint256[](1);
        weights[0] = weight;

        vm.deal(validator, 1);
        ParentFinality memory finality = ParentFinality({height: block.number, blockHash: bytes32(0)});
        // uint64 n = gatewayDiamond.getter().getLastConfigurationNumber() + 1;

        vm.startPrank(FilAddress.SYSTEM_ACTOR);
        gatewayDiamond.topDownFinalizer().commitParentFinality(finality);
        vm.stopPrank();
    }

    function reward(uint256 amount) public view {
        console.log("reward method called with %d", amount);
    }

    function fund(address funderAddress, uint256 fundAmount) public {
        fund(funderAddress, fundAmount, SupplyKind.Native);
    }

    function fund(address funderAddress, uint256 fundAmount, SupplyKind mode) public {
        // funding subnets is free, we do not need cross msg fee
        (SubnetID memory subnetId, , uint256 nonceBefore, , uint256 circSupplyBefore) = getSubnet(address(saDiamond));

        uint256 expectedTopDownMsgsLength = gatewayDiamond.getter().getSubnetTopDownMsgsLength(subnetId) + 1;
        uint256 expectedNonce = nonceBefore + 1;
        uint256 expectedCircSupply = circSupplyBefore + fundAmount;

        if (mode == SupplyKind.Native) {
            gatewayDiamond.manager().fund{value: fundAmount}(subnetId, FvmAddressHelper.from(funderAddress));
        } else if (mode == SupplyKind.ERC20) {
            gatewayDiamond.manager().fundWithToken(subnetId, FvmAddressHelper.from(funderAddress), fundAmount);
        }

        (, , uint256 nonce, , uint256 circSupply) = getSubnet(address(saDiamond));

        require(
            gatewayDiamond.getter().getSubnetTopDownMsgsLength(subnetId) == expectedTopDownMsgsLength,
            "unexpected lengths"
        );

        require(nonce == expectedNonce, "unexpected nonce");
        require(circSupply == expectedCircSupply, "unexpected circSupply");
    }

    function join(address validatorAddress, bytes memory pubkey) public {
        vm.prank(validatorAddress);
        vm.deal(validatorAddress, DEFAULT_COLLATERAL_AMOUNT + 1);
        saDiamond.manager().join{value: DEFAULT_COLLATERAL_AMOUNT}(pubkey);
    }

    function confirmChange(address validator, uint256 privKey) internal {
        address[] memory validators = new address[](1);
        validators[0] = validator;

        uint256[] memory privKeys = new uint256[](1);
        privKeys[0] = privKey;

        confirmChange(validators, privKeys);
    }

    function confirmChange(address validator1, uint256 privKey1, address validator2, uint256 privKey2) internal {
        address[] memory validators = new address[](2);
        validators[0] = validator1;
        validators[1] = validator2;

        uint256[] memory privKeys = new uint256[](2);
        privKeys[0] = privKey1;
        privKeys[1] = privKey2;

        confirmChange(validators, privKeys);
    }

    function confirmChange(
        address validator1,
        uint256 privKey1,
        address validator2,
        uint256 privKey2,
        address validator3,
        uint256 privKey3
    ) internal {
        address[] memory validators = new address[](3);
        validators[0] = validator1;
        validators[1] = validator2;
        validators[2] = validator3;

        uint256[] memory privKeys = new uint256[](3);
        privKeys[0] = privKey1;
        privKeys[1] = privKey2;
        privKeys[2] = privKey3;

        confirmChange(validators, privKeys);
    }

    function confirmChange(address[] memory validators, uint256[] memory privKeys) internal {
        uint256 n = validators.length;

        bytes[] memory signatures = new bytes[](n);

        (uint64 nextConfigNum, ) = saDiamond.getter().getConfigurationNumbers();

        uint256 h = saDiamond.getter().lastBottomUpCheckpointHeight() + saDiamond.getter().bottomUpCheckPeriod();

        BottomUpCheckpoint memory checkpoint = BottomUpCheckpoint({
            subnetID: saDiamond.getter().getParent().createSubnetId(address(saDiamond)),
            blockHeight: h,
            blockHash: keccak256(abi.encode(h)),
            nextConfigurationNumber: nextConfigNum - 1,
            msgs: new IpcEnvelope[](0)
        });

        vm.deal(address(saDiamond), 100 ether);

        bytes32 hash = keccak256(abi.encode(checkpoint));

        for (uint256 i = 0; i < n; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(privKeys[i], hash);
            signatures[i] = abi.encodePacked(r, s, v);
        }

        vm.prank(validators[0]);
        saDiamond.checkpointer().submitCheckpoint(checkpoint, validators, signatures);
    }

    function release(uint256 releaseAmount) public {
        uint256 expectedNonce = gatewayDiamond.getter().bottomUpNonce() + 1;
        gatewayDiamond.manager().release{value: releaseAmount}(FvmAddressHelper.from(msg.sender));
        require(gatewayDiamond.getter().bottomUpNonce() == expectedNonce, "unexpected nonce");
    }

    function addStake(uint256 stakeAmount, address subnetAddress) public {
        uint256 balanceBefore = subnetAddress.balance;

        (, uint256 stakedBefore, , , ) = getSubnet(subnetAddress);

        gatewayDiamond.manager().addStake{value: stakeAmount}();

        uint256 balanceAfter = subnetAddress.balance;
        (, uint256 stakedAfter, , , ) = getSubnet(subnetAddress);

        require(balanceAfter == balanceBefore - stakeAmount, "unexpected balance");
        require(stakedAfter == stakedBefore + stakeAmount, "unexpected stake");
    }

    function registerSubnetGW(uint256 collateral, address subnetAddress, GatewayDiamond gw) public {
        GatewayManagerFacet manager = gw.manager();
        GatewayGetterFacet getter = gw.getter();

        manager.register{value: collateral}(0);

        (SubnetID memory id, uint256 stake, uint256 topDownNonce, , uint256 circSupply) = getSubnetGW(
            subnetAddress,
            gw
        );

        SubnetID memory parentNetwork = getter.getNetworkName();

        require(
            id.toHash() == parentNetwork.createSubnetId(subnetAddress).toHash(),
            "id.toHash() == parentNetwork.createSubnetId(subnetAddress).toHash()"
        );
        require(stake == collateral, "unexpected stake");
        require(topDownNonce == 0, "unexpected nonce");
        require(circSupply == 0, "unexpected circSupply");
    }

    function registerSubnet(uint256 collateral, address subnetAddress) public {
        registerSubnetGW(collateral, subnetAddress, gatewayDiamond);
    }

    function getSubnetCircSupplyGW(SubnetID memory subnetId, GatewayDiamond gw) public view returns (uint256) {
        GatewayGetterFacet getter = gw.getter();
        Subnet memory subnet = getter.subnets(subnetId.toHash());
        return subnet.circSupply;
    }

    function getSubnetGW(
        address subnetAddress,
        GatewayDiamond gw
    ) public view returns (SubnetID memory, uint256, uint256, uint256, uint256) {
        GatewayGetterFacet getter = gw.getter();

        SubnetID memory subnetId = getter.getNetworkName().createSubnetId(subnetAddress);

        Subnet memory subnet = getter.subnets(subnetId.toHash());

        return (subnet.id, subnet.stake, subnet.topDownNonce, subnet.appliedBottomUpNonce, subnet.circSupply);
    }

    function getSubnet(
        address subnetAddress
    ) public view returns (SubnetID memory, uint256, uint256, uint256, uint256) {
        return getSubnetGW(subnetAddress, gatewayDiamond);
    }
}
