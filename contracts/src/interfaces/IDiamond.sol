// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

interface IDiamond {
    // Add=0, Replace=1, Remove=2
    enum FacetCutAction {
        Add,
        Replace,
        Remove
    }

    struct FacetCut {
        address facetAddress;
        FacetCutAction action;
        bytes4[] functionSelectors;
    }
    // The DiamondCut event records all function changes to a diamond.
    event DiamondCut(FacetCut[] _diamondCut, address _init, bytes _calldata);
}
