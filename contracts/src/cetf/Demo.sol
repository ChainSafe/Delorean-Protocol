// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "./CetfAPI.sol";

/// @title A demo contract to show what is possible with Delorean protocol
/// @author BadBoi Labs 
/// @dev The contract accepts funding and will only trigger the validators to release the decrypt key when
///      both the funding goal (88 FIL) and the block height (10) is met
contract DeloreanDemo {

    uint256 constant FUNDING_GOAL = 88 ether;
    uint256 constant BLOCK_HEIGHT_REQUIRED = 10;
    uint64 constant TAG = 0; // this is to allow a contract to manage multiple keys

    error InsufficientFunds();
    error BlockHeightNotReached();

    function releaseKey() public {
        
        // Check the conditions and revert if they are not met
        if (block.number < BLOCK_HEIGHT_REQUIRED) {
            revert BlockHeightNotReached();
        }
        if (address(this).balance < FUNDING_GOAL ) {
            revert InsufficientFunds();
        }

        // All conditions are met so trigger the validators to produce the decryption key
        CetfAPI.enqueueTag(TAG);
        return;
    }
}
