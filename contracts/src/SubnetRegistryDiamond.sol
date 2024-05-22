// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {IDiamond} from "./interfaces/IDiamond.sol";
import {IDiamondCut} from "../src/interfaces/IDiamondCut.sol";
import {IDiamondLoupe} from "./interfaces/IDiamondLoupe.sol";
import {IERC165} from "./interfaces/IERC165.sol";
import {SubnetRegistryActorStorage} from "./lib/LibSubnetRegistryStorage.sol";
import {GatewayCannotBeZero, FacetCannotBeZero} from "./errors/IPCErrors.sol";
import {LibDiamond} from "./lib/LibDiamond.sol";
import {SubnetCreationPrivileges} from "./structs/Subnet.sol";

error FunctionNotFound(bytes4 _functionSelector);

contract SubnetRegistryDiamond {
    SubnetRegistryActorStorage internal s;

    struct ConstructorParams {
        address gateway;
        address getterFacet;
        address managerFacet;
        address rewarderFacet;
        address checkpointerFacet;
        address pauserFacet;
        address diamondCutFacet;
        address diamondLoupeFacet;
        address ownershipFacet;
        bytes4[] subnetActorGetterSelectors;
        bytes4[] subnetActorManagerSelectors;
        bytes4[] subnetActorRewarderSelectors;
        bytes4[] subnetActorCheckpointerSelectors;
        bytes4[] subnetActorPauserSelectors;
        bytes4[] subnetActorDiamondCutSelectors;
        bytes4[] subnetActorDiamondLoupeSelectors;
        bytes4[] subnetActorOwnershipSelectors;
        SubnetCreationPrivileges creationPrivileges;
    }

    constructor(IDiamond.FacetCut[] memory _diamondCut, ConstructorParams memory params) {
        if (params.gateway == address(0)) {
            revert GatewayCannotBeZero();
        }
        if (params.getterFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.managerFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.rewarderFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.checkpointerFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.pauserFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.diamondCutFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.diamondLoupeFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (params.ownershipFacet == address(0)) {
            revert FacetCannotBeZero();
        }

        LibDiamond.setContractOwner(msg.sender);
        LibDiamond.diamondCut({_diamondCut: _diamondCut, _init: address(0), _calldata: new bytes(0)});

        LibDiamond.DiamondStorage storage ds = LibDiamond.diamondStorage();
        // adding ERC165 data
        ds.supportedInterfaces[type(IERC165).interfaceId] = true;
        ds.supportedInterfaces[type(IDiamondCut).interfaceId] = true;
        ds.supportedInterfaces[type(IDiamondLoupe).interfaceId] = true;

        s.GATEWAY = params.gateway;
        s.SUBNET_ACTOR_GETTER_FACET = params.getterFacet;
        s.SUBNET_ACTOR_MANAGER_FACET = params.managerFacet;
        s.SUBNET_ACTOR_REWARD_FACET = params.rewarderFacet;
        s.SUBNET_ACTOR_CHECKPOINTING_FACET = params.checkpointerFacet;
        s.SUBNET_ACTOR_PAUSE_FACET = params.pauserFacet;
        s.SUBNET_ACTOR_DIAMOND_CUT_FACET = params.diamondCutFacet;
        s.SUBNET_ACTOR_LOUPE_FACET = params.diamondLoupeFacet;
        s.SUBNET_ACTOR_OWNERSHIP_FACET = params.ownershipFacet;

        s.subnetActorGetterSelectors = params.subnetActorGetterSelectors;
        s.subnetActorManagerSelectors = params.subnetActorManagerSelectors;
        s.subnetActorRewarderSelectors = params.subnetActorRewarderSelectors;
        s.subnetActorCheckpointerSelectors = params.subnetActorCheckpointerSelectors;
        s.subnetActorPauserSelectors = params.subnetActorPauserSelectors;
        s.subnetActorDiamondCutSelectors = params.subnetActorDiamondCutSelectors;
        s.subnetActorDiamondLoupeSelectors = params.subnetActorDiamondLoupeSelectors;
        s.subnetActorOwnershipSelectors = params.subnetActorOwnershipSelectors;

        s.creationPrivileges = params.creationPrivileges;
    }

    function _fallback() internal {
        LibDiamond.DiamondStorage storage ds;
        bytes32 position = LibDiamond.DIAMOND_STORAGE_POSITION;
        // get diamond storage
        // slither-disable-next-line assembly
        assembly {
            ds.slot := position
        }
        // get facet from function selector
        address facet = ds.facetAddressAndSelectorPosition[msg.sig].facetAddress;
        if (facet == address(0)) {
            revert FunctionNotFound(msg.sig);
        }
        // Execute external function from facet using delegatecall and return any value.
        // slither-disable-next-line assembly
        assembly {
            // copy function selector and any arguments
            calldatacopy(0, 0, calldatasize())
            // execute function call using the facet
            let result := delegatecall(gas(), facet, 0, calldatasize(), 0, 0)
            // get any return value
            returndatacopy(0, 0, returndatasize())
            // return any return value or error back to the caller
            switch result
            case 0 {
                revert(0, returndatasize())
            }
            default {
                return(0, returndatasize())
            }
        }
    }

    /// @notice Will run when no functions matches call data
    fallback() external payable {
        _fallback();
    }

    /// @notice Same as fallback but called when calldata is empty
    receive() external payable {
        _fallback();
    }
}
