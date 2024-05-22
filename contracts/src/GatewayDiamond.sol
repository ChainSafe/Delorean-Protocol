// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {GatewayActorStorage} from "./lib/LibGatewayActorStorage.sol";
import {IDiamond} from "./interfaces/IDiamond.sol";
import {IDiamondCut} from "./interfaces/IDiamondCut.sol";
import {IDiamondLoupe} from "./interfaces/IDiamondLoupe.sol";
import {IERC165} from "./interfaces/IERC165.sol";
import {Validator, Membership} from "./structs/Subnet.sol";
import {InvalidCollateral, InvalidSubmissionPeriod, InvalidMajorityPercentage} from "./errors/IPCErrors.sol";
import {LibDiamond} from "./lib/LibDiamond.sol";
import {LibGateway} from "./lib/LibGateway.sol";
import {SubnetID} from "./structs/Subnet.sol";
import {LibStaking} from "./lib/LibStaking.sol";
import {BATCH_PERIOD, MAX_MSGS_PER_BATCH} from "./structs/CrossNet.sol";

error FunctionNotFound(bytes4 _functionSelector);

bool constant FEATURE_MULTILEVEL_CROSSMSG = false;
bool constant FEATURE_GENERAL_PUPRPOSE_CROSSMSG = true;
uint8 constant FEATURE_SUBNET_DEPTH = 2;

contract GatewayDiamond {
    GatewayActorStorage internal s;

    struct ConstructorParams {
        uint256 bottomUpCheckPeriod;
        uint16 activeValidatorsLimit;
        uint8 majorityPercentage;
        SubnetID networkName;
        Validator[] genesisValidators;
        bytes32 commitSha;
    }

    constructor(IDiamond.FacetCut[] memory _diamondCut, ConstructorParams memory params) {
        // The bottomUpCheckPeriod should be non-zero for now.
        if (params.bottomUpCheckPeriod == 0) {
            revert InvalidSubmissionPeriod();
        }

        if (params.majorityPercentage < 51 || params.majorityPercentage > 100) {
            revert InvalidMajorityPercentage();
        }

        LibDiamond.setContractOwner(msg.sender);
        LibDiamond.diamondCut({_diamondCut: _diamondCut, _init: address(0), _calldata: new bytes(0)});

        LibDiamond.DiamondStorage storage ds = LibDiamond.diamondStorage();
        // adding ERC165 data
        ds.supportedInterfaces[type(IERC165).interfaceId] = true;
        ds.supportedInterfaces[type(IDiamondCut).interfaceId] = true;
        ds.supportedInterfaces[type(IDiamondLoupe).interfaceId] = true;

        // Feature flags
        s.maxTreeDepth = FEATURE_SUBNET_DEPTH;
        s.generalPurposeCrossMsg = FEATURE_GENERAL_PUPRPOSE_CROSSMSG;
        s.multiLevelCrossMsg = FEATURE_MULTILEVEL_CROSSMSG;

        s.networkName = params.networkName;
        s.bottomUpCheckPeriod = params.bottomUpCheckPeriod;
        s.majorityPercentage = params.majorityPercentage;
        s.checkpointQuorumMap.retentionHeight = 1;
        s.commitSha = params.commitSha;

        // BottomUpMsgBatch config parameters.
        // NOTE: Let's fix them for now, but we could make them configurable
        // through the gateway constructor in the future.
        s.maxMsgsPerBottomUpBatch = MAX_MSGS_PER_BATCH;

        s.validatorsTracker.validators.activeLimit = params.activeValidatorsLimit;
        // Start the next configuration number from 1, 0 is reserved for no change and the genesis membership
        s.validatorsTracker.changes.nextConfigurationNumber = LibStaking.INITIAL_CONFIGURATION_NUMBER;
        // The startConfiguration number is also 1 to match with nextConfigurationNumber, indicating we have
        // empty validator change logs
        s.validatorsTracker.changes.startConfigurationNumber = LibStaking.INITIAL_CONFIGURATION_NUMBER;
        // set initial validators and update membership
        Membership memory initial = Membership({configurationNumber: 0, validators: params.genesisValidators});
        LibGateway.updateMembership(initial);
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
