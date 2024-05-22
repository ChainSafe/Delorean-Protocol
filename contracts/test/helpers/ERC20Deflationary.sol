// SPDX-License-Identifier: MIT
pragma solidity ^0.8.21;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract ERC20Deflationary is ERC20, Ownable {
    uint256 public deflationRatePercentage;

    constructor(
        string memory name_,
        string memory symbol_,
        uint256 initialSupply,
        address owner,
        uint256 deflationRate_
    ) ERC20(name_, symbol_) Ownable(owner) {
        require(deflationRate_ <= 100, "Deflation rate must be between 0 and 100.");
        deflationRatePercentage = deflationRate_;
        _mint(owner, initialSupply);
    }

    function setDeflationRate(uint256 newRate) public onlyOwner {
        require(newRate <= 100, "Deflation rate must be between 0 and 100.");
        deflationRatePercentage = newRate;
    }

    function _update(address from, address to, uint256 value) internal virtual override {
        if (from != address(0) && to != address(0)) {
            uint256 burnAmount = (value * deflationRatePercentage) / 100;
            uint256 transferAmount = value - burnAmount;

            super._update(from, to, transferAmount); // Perform the standard transfer with the reduced amount.

            if (burnAmount > 0) {
                _burn(from, burnAmount); // Burn the deflation amount from the sender.
            }
        } else {
            super._update(from, to, value);
        }
    }
}
