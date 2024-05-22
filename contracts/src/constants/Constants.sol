// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

address constant BURNT_FUNDS_ACTOR = address(99);
bytes32 constant EMPTY_HASH = bytes32("");
bytes constant EMPTY_BYTES = bytes("");
bytes4 constant METHOD_SEND = bytes4(0);

// The length of the public key that is associated with a validator.
uint256 constant VALIDATOR_SECP256K1_PUBLIC_KEY_LENGTH = 65;
