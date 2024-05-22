// SPDX-License-Identifier: MIT
pragma solidity ^0.8.21;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract ERC20Inflationary is ERC20, Ownable {
    uint256 public inflationRatePercentage;

    constructor(
        string memory name_,
        string memory symbol_,
        uint256 initialSupply,
        address owner,
        uint256 inflationRate_
    ) ERC20(name_, symbol_) Ownable(owner) {
        require(inflationRate_ <= 100, "Inflation rate must be between 0 and 100.");
        inflationRatePercentage = inflationRate_;
        _mint(owner, initialSupply);
    }

    function setInflationRate(uint256 newRate) public onlyOwner {
        require(newRate <= 100, "Inflation rate must be between 0 and 100.");
        inflationRatePercentage = newRate;
    }

    function _update(address from, address to, uint256 value) internal virtual override {
        if (from != address(0) && to != address(0)) {
            super._update(from, to, value); // Perform the standard transfer first.

            uint256 inflationAmount = (value * inflationRatePercentage) / 100;
            _mint(to, inflationAmount); // Mint the inflation to the recipient.
        } else {
            super._update(from, to, value);
        }
    }
}
