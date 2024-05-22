// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import { IInterchainTokenService } from "@axelar-network/interchain-token-service/interfaces/IInterchainTokenService.sol";
import { AddressBytes } from "@axelar-network/axelar-gmp-sdk-solidity/contracts/libs/AddressBytes.sol";
import { IERC20 } from "openzeppelin-contracts/interfaces/IERC20.sol";
import { SubnetID } from "@ipc/src/structs/Subnet.sol";

// @notice The IpcTokenSender can be deployed in an Axelar-supported L1 containing the canonical version of some ERC20
//         token (e.g. Ethereum, Polygon, etc.) we want to transfer to an token-supply IPC subnet anchored on another
//         Axelar-supported L1 (e.g. Filecoin). The duo of IpcTokenSender and IpcTokenkHandler achieve this in a single
//         atomic step.
contract IpcTokenSender {
    IInterchainTokenService public immutable _axelarIts;
    string public _destinationChain;
    bytes public _destinationTokenHandler;

    constructor(address axelarIts, string memory destinationChain, address destinationTokenHandler) {
        _axelarIts = IInterchainTokenService(axelarIts);
        _destinationChain = destinationChain;
        _destinationTokenHandler = AddressBytes.toBytes(destinationTokenHandler);
    }

    function fundSubnet(bytes32 tokenId, SubnetID calldata subnet, address recipient, uint256 amount) external payable {
        require(msg.value > 0, "gas payment is required");

        // Retrieve the token address from the Axelar ITS.
        address tokenAddress = _axelarIts.validTokenAddress(tokenId);
        require(tokenAddress != address(0), "could not resolve token address");

        IERC20 token = IERC20(tokenAddress);

        // Perform some sanity checks.
        require(token.balanceOf(msg.sender) >= amount, "insufficient token balance");
        require(token.allowance(msg.sender, address(this)) >= amount, "insufficient token allowance");

        // Lock the value under custody, and authorize the Axelar ITS to spend it on our behalf.
        token.transferFrom(msg.sender, address(this), amount);
        token.approve(address(_axelarIts), amount);

        // Tell the IpcTokenHandler on the IPC L1 rootnet to credit these funds to the specified beneficiary
        // in the designated subnet.
        bytes memory payload = abi.encode(subnet, recipient);
        _axelarIts.callContractWithInterchainToken{ value: msg.value }(
            tokenId,
            _destinationChain,
            _destinationTokenHandler,
            amount,
            payload,
            msg.value
        );
    }
}