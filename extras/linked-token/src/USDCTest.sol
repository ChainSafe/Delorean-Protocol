// SPDX-License-Identifier: MIT
pragma solidity 0.8.23;

import {ERC20} from "openzeppelin-contracts/token/ERC20/ERC20.sol";
import {Ownable} from "openzeppelin-contracts/access/Ownable.sol";

contract USDCTest is ERC20, Ownable {
    constructor() ERC20("USDC", "USDC") Ownable(msg.sender) {}

    function mint(uint256 amount) public onlyOwner {
        _mint(msg.sender, amount);
    }
}
