// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import { InterchainTokenExecutable } from '@axelar-network/interchain-token-service/executable/InterchainTokenExecutable.sol';
import { IERC20 } from "openzeppelin-contracts/interfaces/IERC20.sol";
import { Ownable } from "openzeppelin-contracts/access/Ownable.sol";
import { SubnetID, SupplySource, SupplyKind } from "@ipc/src/structs/Subnet.sol";
import { FvmAddress } from "@ipc/src/structs/FvmAddress.sol";
import { IIpcHandler } from "@ipc/sdk/interfaces/IIpcHandler.sol";
import { IpcMsgKind, ResultMsg, OutcomeType, IpcEnvelope } from "@ipc/src/structs/CrossNet.sol";
import { FvmAddressHelper } from "@ipc/src/lib/FvmAddressHelper.sol";
import { SubnetIDHelper } from "@ipc/src/lib/SubnetIDHelper.sol";
import { SafeERC20 } from "openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";

interface TokenFundedGateway {
    function fundWithToken(SubnetID calldata subnetId, FvmAddress calldata to, uint256 amount) external;
}

interface SubnetActor {
    function supplySource() external returns (SupplySource memory supply);
}

// @notice The IpcTokenHandler sits in an Axelar-supported L1 housing an IPC subnet hierarchy. It is invoked by the
//         IpcTokenSender via the Axelar ITS, receiving some token value to deposit into an IPC subnet (specified in the
//         incoming message). The IpcTokenHandler handles deposit failures by crediting the value back to the original
//         beneficiary, and making it available from them to withdraw() on the rootnet.
contract IpcTokenHandler is InterchainTokenExecutable, IIpcHandler, Ownable {
    using FvmAddressHelper for address;
    using FvmAddressHelper for FvmAddress;
    using SubnetIDHelper for SubnetID;
    using SafeERC20 for IERC20;

    error NothingToWithdraw();

    event SubnetFunded(SubnetID indexed subnet, address indexed recipient, uint256 value);
    event FundingFailed(SubnetID indexed subnet, address indexed recipient, uint256 value);

    TokenFundedGateway public _ipcGateway;

    constructor(address axelarIts, address ipcGateway, address admin) InterchainTokenExecutable(axelarIts) Ownable(admin) {
        _ipcGateway = TokenFundedGateway(ipcGateway);
    }

    // @notice The InterchainTokenExecutable abstract parent contract hands off to this function after verifying that
    //         the call originated at the Axelar ITS.
    function _executeWithInterchainToken(
        bytes32, // commandId
        string calldata, // sourceChain
        bytes calldata, // sourceAddress
        bytes calldata data,
        bytes32, // tokenId
        address tokenAddr,
        uint256 amount
    ) internal override {
        IERC20 token = IERC20(tokenAddr);
        require(token.balanceOf(address(this)) >= amount, "insufficient balance");

        // Authorize the IPC gateway to spend these tokens on our behalf.
        token.safeIncreaseAllowance(address(_ipcGateway), amount);

        // Try to decode the payload. Note: Solidity does not support try/catch for abi.decode (or tryDecode), so
        // this may fail if there's a bug in the sender (in which case funds can be retrieved through the admin path).
        (SubnetID memory subnet, address recipient) = abi.decode(data, (SubnetID, address));

        (bool success, ) = address(_ipcGateway).call(
            abi.encodeWithSelector(TokenFundedGateway.fundWithToken.selector, subnet, recipient.from(), amount)
        );

        if (!success) {
            // Restore the original allowance.
            token.safeDecreaseAllowance(address(_ipcGateway), amount);

            // Increase the allowance of the admin address so they can retrieve these otherwise lost tokens.
            token.safeIncreaseAllowance(owner(), amount);

            // Emit a FundingFailed event.
            emit FundingFailed(subnet, recipient, amount);

            return;
        }

        emit SubnetFunded(subnet, recipient, amount);
    }

    // @notice Handles result messages for funding operations.
    function handleIpcMessage(IpcEnvelope calldata envelope) external payable returns (bytes memory ret) {
        if (msg.sender != address(_ipcGateway)) {
            revert IIpcHandler.CallerIsNotGateway();
        }
        if (envelope.kind != IpcMsgKind.Result) {
            revert IIpcHandler.UnsupportedMsgKind();
        }

        ResultMsg memory result = abi.decode(envelope.message, (ResultMsg));
        if (result.outcome != OutcomeType.Ok) {
            // Verify that the subnet is indeed an ERC20 subnet.
            SupplySource memory supplySource = SubnetActor(envelope.from.subnetId.getAddress()).supplySource();
            require(supplySource.kind == SupplyKind.ERC20, "expected ERC20 supply source");

            // Increase the allowance of the admin address so they can retrieve these otherwise lost tokens.
            IERC20(supplySource.tokenAddress).safeIncreaseAllowance(owner(), envelope.value);

            // Results will carry the original beneficiary in the 'from' address.
            address beneficiary = envelope.from.rawAddress.extractEvmAddress();

            // Emit an event.
            emit FundingFailed(envelope.from.subnetId, beneficiary, envelope.value);
        }

        return bytes("");
    }

    // @notice The ultimate backstop in case the error-handling logic itself failed unexpectedly and we failed to
    //         increase the recovery allowances of the admin address.
    function adminTokenIncreaseAllowance(address token, uint256 amount) external onlyOwner {
        IERC20(token).safeIncreaseAllowance(owner(), amount);
    }

}