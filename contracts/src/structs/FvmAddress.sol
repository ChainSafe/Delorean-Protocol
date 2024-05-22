// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

/*
 * @dev The corresponding implementation of Fil Address from FVM.
 * Currently it supports only f1 addresses.
 * See: https://github.com/filecoin-project/ref-fvm/blob/db8c0b12c801f364e87bda6f52d00c6bd0e1b878/shared/src/address/payload.rs#L87
 */
struct FvmAddress {
    uint8 addrType;
    bytes payload;
}

/*
 * @dev The delegated f4 address in Fil Address from FVM.
 */
struct DelegatedAddress {
    uint64 namespace;
    uint128 length;
    bytes buffer;
}
