// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.20;

import "openzeppelin-contracts/token/ERC20/ERC20.sol";
import "openzeppelin-contracts/access/Ownable.sol";

contract DummyERC20 is ERC20, Ownable {
    constructor(
        string memory _name,
        string memory _symbol,
        uint256 _initialSupply
    ) Ownable(msg.sender) ERC20(_name, _symbol) {
        _mint(owner(), _initialSupply);
    }

    function mint(address _to, uint256 _amount) public onlyOwner {
        _mint(_to, _amount);
    }
}
