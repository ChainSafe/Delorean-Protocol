// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/StdUtils.sol";
import "forge-std/StdCheats.sol";
import {CommonBase} from "forge-std/Base.sol";
import {RegisterSubnetFacet} from "../../../src/subnetregistry/RegisterSubnetFacet.sol";
import {SubnetGetterFacet} from "../../../src/subnetregistry/SubnetGetterFacet.sol";
import {SubnetActorDiamond} from "../../../src/SubnetActorDiamond.sol";
import {SubnetRegistryDiamond} from "../../../src/SubnetRegistryDiamond.sol";
import {ConsensusType} from "../../../src/enums/ConsensusType.sol";
import {SubnetID, PermissionMode} from "../../../src/structs/Subnet.sol";
import {SupplySourceHelper} from "../../../src/lib/SupplySourceHelper.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";
import {RegistryFacetsHelper} from "../../helpers/RegistryFacetsHelper.sol";

contract SubnetRegistryHandler is CommonBase, StdCheats, StdUtils {
    using EnumerableSet for EnumerableSet.AddressSet;
    using RegistryFacetsHelper for SubnetRegistryDiamond;

    address private constant DEFAULT_IPC_GATEWAY_ADDR = address(1024);
    uint64 constant DEFAULT_CHECKPOINT_PERIOD = 10;
    uint256 private constant DEFAULT_MIN_VALIDATOR_STAKE = 1 ether;
    uint8 private constant DEFAULT_MAJORITY_PERCENTAGE = 70;
    int8 private constant DEFAULT_POWER_SCALE = 18;
    uint64 private constant ROOTNET_CHAINID = 123;
    uint64 private constant DEFAULT_MIN_VALIDATORS = 1;
    uint16 private constant DEFAULT_ACTIVE_VALIDATORS = 50;
    uint256 private constant CROSS_MSG_FEE = 10 gwei;

    EnumerableSet.AddressSet private ghost_owners;
    RegisterSubnetFacet private registerSubnetFacet;
    SubnetGetterFacet private registerGetterFacet;

    address private registerSubnetFacetAddr;
    address private subnetGetterFacetAddr;

    constructor(SubnetRegistryDiamond _registry) {
        registerSubnetFacet = _registry.register();
        registerGetterFacet = _registry.getter();
    }

    function getSubnetDeployedBy(address owner) external view returns (address subnet) {
        return registerGetterFacet.latestSubnetDeployed(owner);
    }

    function getSubnetDeployedWithNonce(address owner, uint64 nonce) external view returns (address subnet) {
        return registerGetterFacet.getSubnetDeployedByNonce(owner, nonce);
    }

    function getUserLastNonce(address user) external view returns (uint64 nonce) {
        return registerGetterFacet.getUserLastNonce(user);
    }

    /// getRandomOldAddressOrNewOne returns a new random address
    function getRandomOldAddressOrNewOne(uint256 seed) internal view returns (address) {
        uint256 lenght = ghost_owners.length();
        if (lenght == 0 || seed % 4 == 0) {
            return msg.sender;
        } else {
            return ghost_owners.values()[seed % lenght];
        }
    }

    function getOwners() external view returns (address[] memory) {
        return ghost_owners.values();
    }

    function getGateway() external view returns (address) {
        return registerGetterFacet.getGateway();
    }

    function deploySubnetActorFromRegistry(
        uint256 _minCollateral,
        uint64 _minValidators,
        uint64 _bottomUpCheckPeriod,
        uint16 _activeValidatorsLimit,
        uint8 _majorityPercentage,
        uint256 _minCrossMsgFee,
        uint8 _pathSize,
        int8 _powerScale,
        uint256 seed
    ) public {
        if (_minCollateral > DEFAULT_MIN_VALIDATOR_STAKE || _minCollateral == 0) {
            _minCollateral = DEFAULT_MIN_VALIDATOR_STAKE;
        }
        if (_bottomUpCheckPeriod > DEFAULT_CHECKPOINT_PERIOD || _bottomUpCheckPeriod == 0) {
            _bottomUpCheckPeriod = DEFAULT_CHECKPOINT_PERIOD;
        }
        if (_majorityPercentage < 51 || _majorityPercentage > 100) {
            _majorityPercentage = DEFAULT_MAJORITY_PERCENTAGE;
        }
        if (_powerScale > DEFAULT_POWER_SCALE) {
            _powerScale = DEFAULT_POWER_SCALE;
        }
        if (_minValidators > DEFAULT_MIN_VALIDATORS || _minValidators == 0) {
            _minValidators = DEFAULT_MIN_VALIDATORS;
        }
        if (_pathSize > 5) {
            _pathSize = 1;
        }
        if (_minCrossMsgFee > 1 ether || _minCrossMsgFee == 0) {
            _minCrossMsgFee = CROSS_MSG_FEE;
        }
        if (_activeValidatorsLimit > DEFAULT_ACTIVE_VALIDATORS || _activeValidatorsLimit == 0) {
            _activeValidatorsLimit = DEFAULT_ACTIVE_VALIDATORS;
        }

        address[] memory path = new address[](_pathSize);
        for (uint256 i; i < _pathSize; ++i) {
            path[i] = address(uint160(i));
        }

        SubnetActorDiamond.ConstructorParams memory params = SubnetActorDiamond.ConstructorParams({
            parentId: SubnetID({root: ROOTNET_CHAINID, route: path}),
            ipcGatewayAddr: registerGetterFacet.getGateway(),
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

        address owner = getRandomOldAddressOrNewOne(seed);
        vm.prank(owner);
        registerSubnetFacet.newSubnetActor(params);
        ghost_owners.add(owner);
    }
}
