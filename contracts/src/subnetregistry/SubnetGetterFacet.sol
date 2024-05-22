// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;
import {SubnetRegistryActorStorage} from "../lib/LibSubnetRegistryStorage.sol";
import {CannotFindSubnet, FacetCannotBeZero} from "../errors/IPCErrors.sol";
import {LibDiamond} from "../lib/LibDiamond.sol";

contract SubnetGetterFacet {
    // slither-disable-next-line uninitialized-state
    SubnetRegistryActorStorage internal s;

    /// @notice Returns the address of the latest subnet actor deployed by a user.
    /// @param owner The address of the user whose latest subnet deployment is queried.
    function latestSubnetDeployed(address owner) external view returns (address subnet) {
        uint64 nonce = s.userNonces[owner];
        if (nonce == 0) {
            revert CannotFindSubnet();
        }

        subnet = s.subnets[owner][nonce];
        if (subnet == address(0)) {
            revert CannotFindSubnet();
        }
    }

    /// @notice Returns the address of a subnet actor deployed for a specific nonce by a user.
    /// @param owner The address of the user whose subnet deployment is queried.
    /// @param nonce The specific nonce associated with the subnet deployment.
    function getSubnetDeployedByNonce(address owner, uint64 nonce) external view returns (address subnet) {
        if (nonce == 0) {
            revert CannotFindSubnet();
        }
        subnet = s.subnets[owner][nonce];
        if (subnet == address(0)) {
            revert CannotFindSubnet();
        }
    }

    /// @notice Returns the last nonce used by the owner.
    /// @param user The address of the user whose last nonce is being queried.
    function getUserLastNonce(address user) external view returns (uint64 nonce) {
        nonce = s.userNonces[user];
        if (nonce == 0) {
            revert CannotFindSubnet();
        }
    }

    /// @notice Returns the gateway.
    function getGateway() external view returns (address) {
        return s.GATEWAY;
    }

    /// @notice Returns the address of the SUBNET_GETTER_FACET.
    function getSubnetActorGetterFacet() external view returns (address) {
        return s.SUBNET_ACTOR_GETTER_FACET;
    }

    /// @notice Returns the address of the SUBNET_MANAGER_FACET.
    function getSubnetActorManagerFacet() external view returns (address) {
        return s.SUBNET_ACTOR_MANAGER_FACET;
    }

    /// @notice Returns the address of the SUBNET_ACTOR_REWARDER_FACET.
    function getSubnetActorRewarderFacet() external view returns (address) {
        return s.SUBNET_ACTOR_REWARD_FACET;
    }

    /// @notice Returns the address of the SUBNET_ACTOR_CHECKPOINTER_FACET.
    function getSubnetActorCheckpointerFacet() external view returns (address) {
        return s.SUBNET_ACTOR_CHECKPOINTING_FACET;
    }

    /// @notice Returns the address of the SUBNET_ACTOR_PAUSER_FACET.
    function getSubnetActorPauserFacet() external view returns (address) {
        return s.SUBNET_ACTOR_PAUSE_FACET;
    }

    /// @notice Returns the subnet actor getter selectors.
    function getSubnetActorGetterSelectors() external view returns (bytes4[] memory) {
        return s.subnetActorGetterSelectors;
    }

    /// @notice Returns the subnet actor manager selectors.
    function getSubnetActorManagerSelectors() external view returns (bytes4[] memory) {
        return s.subnetActorManagerSelectors;
    }

    /// @notice Returns the subnet actor rewarder selectors.
    function getSubnetActorRewarderSelectors() external view returns (bytes4[] memory) {
        return s.subnetActorRewarderSelectors;
    }

    /// @notice Returns the subnet actor checkpointer selectors.
    function getSubnetActorCheckpointerSelectors() external view returns (bytes4[] memory) {
        return s.subnetActorCheckpointerSelectors;
    }

    /// @notice Returns the subnet actor pauser selectors.
    function getSubnetActorPauserSelectors() external view returns (bytes4[] memory) {
        return s.subnetActorPauserSelectors;
    }

    /// @notice Updates references to the subnet contract components, including facets and selector sets.
    /// Only callable by the contract owner.
    /// @param newGetterFacet The address of the new subnet getter facet.
    /// @param newManagerFacet The address of the new subnet manager facet.
    /// @param newSubnetGetterSelectors An array of function selectors for the new subnet getter facet.
    /// @param newSubnetManagerSelectors An array of function selectors for the new subnet manager facet.
    function updateReferenceSubnetContract(
        address newGetterFacet,
        address newManagerFacet,
        bytes4[] calldata newSubnetGetterSelectors,
        bytes4[] calldata newSubnetManagerSelectors
    ) external {
        LibDiamond.enforceIsContractOwner();

        // Validate addresses are not zero
        if (newGetterFacet == address(0)) {
            revert FacetCannotBeZero();
        }
        if (newManagerFacet == address(0)) {
            revert FacetCannotBeZero();
        }

        // Update the storage variables
        s.SUBNET_ACTOR_GETTER_FACET = newGetterFacet;
        s.SUBNET_ACTOR_MANAGER_FACET = newManagerFacet;

        s.subnetActorGetterSelectors = newSubnetGetterSelectors;
        s.subnetActorManagerSelectors = newSubnetManagerSelectors;
    }
}
