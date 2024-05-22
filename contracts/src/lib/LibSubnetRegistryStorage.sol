// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {SubnetCreationPrivileges} from "../structs/Subnet.sol";

struct SubnetRegistryActorStorage {
    // solhint-disable-next-line var-name-mixedcase
    address GATEWAY;
    /// The getter and manager facet shared by diamond
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_GETTER_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_MANAGER_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_REWARD_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_CHECKPOINTING_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_PAUSE_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_DIAMOND_CUT_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_LOUPE_FACET;
    // solhint-disable-next-line var-name-mixedcase
    address SUBNET_ACTOR_OWNERSHIP_FACET;
    /// The subnet actor getter facet functions selectors
    bytes4[] subnetActorGetterSelectors;
    /// The subnet actor manager facet functions selectors
    bytes4[] subnetActorManagerSelectors;
    /// The subnet actor reward facet functions selectors
    bytes4[] subnetActorRewarderSelectors;
    /// The subnet actor checkpointing facet functions selectors
    bytes4[] subnetActorCheckpointerSelectors;
    /// The subnet actor pause facet functions selectors
    bytes4[] subnetActorPauserSelectors;
    /// The subnet actor diamond cut facet functions selectors
    bytes4[] subnetActorDiamondCutSelectors;
    /// The subnet actor loupe facet functions selectors
    bytes4[] subnetActorDiamondLoupeSelectors;
    /// The subnet actor ownership facet functions selectors
    bytes4[] subnetActorOwnershipSelectors;
    /// @notice Mapping that tracks the deployed subnet actors per user.
    /// Key is the hash of Subnet ID, values are addresses.
    /// mapping owner => nonce => subnet
    mapping(address => mapping(uint64 => address)) subnets;
    /// @notice Mapping that tracks the latest nonce of the deployed
    /// subnet for each user.
    /// owner => nonce
    mapping(address => uint64) userNonces;
    /// @notice The subnet creation privileges.
    SubnetCreationPrivileges creationPrivileges;
}
