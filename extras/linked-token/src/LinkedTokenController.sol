// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.23;

import {SafeERC20} from "openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";
import {IERC20} from "openzeppelin-contracts/token/ERC20/IERC20.sol";
import {LinkedToken} from "./LinkedToken.sol";
import {SubnetID} from "@ipc/src/structs/Subnet.sol";
import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";

contract LinkedTokenController is Initializable, LinkedToken, UUPSUpgradeable {
    using SafeERC20 for IERC20;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address gateway,
        address underlyingToken,
        SubnetID memory linkedSubnet,
        address linkedContract
    ) public initializer {
        __LinkedToken_init(gateway, underlyingToken, linkedSubnet, linkedContract);
        __UUPSUpgradeable_init();
    }

    // upgrade proxy - onlyOwner can upgrade
    // owner is set in inherited initializer -> __LinkedToken_init -> __IpcExchangeUpgradeable_init
    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}

    function _captureTokens(address holder, uint256 amount) internal override {
        _underlying.safeTransferFrom({from: msg.sender, to: address(this), value: amount});
    }

    function _releaseTokens(address beneficiary, uint256 amount) internal override {
        _underlying.safeTransfer(beneficiary, amount);
    }
}
