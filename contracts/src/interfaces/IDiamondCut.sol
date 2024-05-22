// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {IDiamond} from "./IDiamond.sol";

interface IDiamondCut is IDiamond {
    /**
     * @notice Add/replace/remove any number of functions and optionally execute a function with delegatecall
     * @param _diamondCut Contains the facet addresses and function selectors
     * @param _init The address of the contract or facet to execute _calldata
     * @param _calldata A function call, including function selector and arguments _calldata is executed with delegatecall on `_init`
     */
    function diamondCut(IDiamond.FacetCut[] calldata _diamondCut, address _init, bytes calldata _calldata) external;
}
