// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "./DeloreanAPI.sol";

/// @title A demo contract to show what is possible with Delorean protocol
/// @author BadBoi Labs 
/// @dev The contract accepts funding and will only trigger the validators to release the decrypt key when
///      both the funding goal (88 FIL) and the block height (10) is met
contract DeloreanDemo {

    uint256 constant FUNDING_GOAL = 88 ether;
    bytes32 constant MEMO = 0x1111111111111111111111111111111111111111111111111111111111111111; // this is to allow a contract to manage multiple keys

    error InsufficientFunds();

    function releaseKey() public returns (bool) {
        
        // // Check the conditions and revert if they are not met
        if (address(this).balance < FUNDING_GOAL ) {
            revert InsufficientFunds();
        }

        // All conditions are met so trigger the validators to produce the decryption key
        DeloreanAPI.enqueueTag(MEMO);
        return (true);
    }

    // Helper function to allow retrieving the bytes32 tag that the validators will be signing
    // which includes the contract address as well as the variable tag component
    function signingTag() public view returns (bytes32) {
        return keccak256(abi.encodePacked(address(this), MEMO));
    }

    // Required so the contract can receive funds
    receive() external payable  { }
}
