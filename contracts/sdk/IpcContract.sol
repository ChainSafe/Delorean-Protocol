// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {IpcEnvelope, ResultMsg, CallMsg, IpcMsgKind} from "../src/structs/CrossNet.sol";
import {IPCAddress} from "../src/structs/Subnet.sol";
import {EMPTY_BYTES} from "../src/constants/Constants.sol";
import {IGateway} from "../src/interfaces/IGateway.sol";
import {ReentrancyGuard} from "openzeppelin-contracts/utils/ReentrancyGuard.sol";
import {Ownable} from "openzeppelin-contracts/access/Ownable.sol";
import {CrossMsgHelper} from "../src/lib/CrossMsgHelper.sol";
import {IIpcHandler} from "./interfaces/IIpcHandler.sol";

abstract contract IpcExchange is IIpcHandler, Ownable, ReentrancyGuard {
    using CrossMsgHelper for IpcEnvelope;

    // The address of the gateway in the network.
    address public immutable gatewayAddr;

    // List of messages in-flight for which the contract hasn't received a receipt yet.
    mapping(bytes32 => IpcEnvelope) public inflightMsgs;

    constructor(address gatewayAddr_) Ownable(msg.sender) {
        gatewayAddr = gatewayAddr_;
    }

    /// @notice Entrypoint for IPC-enabled contracts. This function is always called by
    /// the gateway when a `Call` or `Receipt` cross-net messages is targeted to
    /// a specific address in the subnet.
    function handleIpcMessage(IpcEnvelope calldata envelope) external payable onlyGateway returns (bytes memory) {
        // internal dispatch of the cross-net message to the right method.
        if (envelope.kind == IpcMsgKind.Call) {
            CallMsg memory call = abi.decode(envelope.message, (CallMsg));
            return _handleIpcCall(envelope, call);
        } else if (envelope.kind == IpcMsgKind.Result) {
            ResultMsg memory result = abi.decode(envelope.message, (ResultMsg));

            // Recover the original message.
            // If we were not tracking it, or if some details don't match, refuse to handle the receipt.
            IpcEnvelope storage orig = inflightMsgs[result.id];
            if (orig.message.length == 0 || keccak256(abi.encode(envelope.from)) != keccak256(abi.encode(orig.to))) {
                revert IIpcHandler.UnrecognizedResult();
            }

            /// Note: if the result handler reverts, we will
            _handleIpcResult(orig, envelope, result);
            delete inflightMsgs[result.id];
            return EMPTY_BYTES;
        }
        revert UnsupportedMsgKind();
    }

    /// @notice Function to be overridden by the child contract to handle incoming IPC calls.
    ///
    /// NOTE: It's fine for this method to revert. If that happens, IPC will carry the error to the caller.
    function _handleIpcCall(
        IpcEnvelope memory envelope,
        CallMsg memory callMsg
    ) internal virtual returns (bytes memory);

    /// @notice Function to be overridden by the child contract to handle results from previously performed IPC calls.
    ///
    /// NOTE: This must not revert as doing so will leave the correlation map in an inconsistent state.
    /// (IPC will consider the result delivery attempted, and will not repeat it again).
    function _handleIpcResult(
        IpcEnvelope storage original,
        IpcEnvelope memory result,
        ResultMsg memory resultMsg
    ) internal virtual;

    /// @notice Method the implementation of this contract can invoke to perform an IPC call.
    function performIpcCall(
        IPCAddress memory to,
        CallMsg memory callMsg,
        uint256 value
    ) internal nonReentrant returns (IpcEnvelope memory envelope) {
        // Queue the cross-net message for dispatch.
        envelope = IGateway(gatewayAddr).sendContractXnetMessage{value: value}(
            IpcEnvelope({
                kind: IpcMsgKind.Call,
                from: to, // TODO: will anyway be replaced by sendContractXnetMessage.
                to: to,
                nonce: 0, // TODO: will be replaced.
                value: value,
                message: abi.encode(callMsg)
            })
        );
        // Add the message to the list of inflights
        bytes32 id = envelope.toHash();
        inflightMsgs[id] = envelope;
    }

    function dropMessages(bytes32[] calldata ids) public onlyOwner {
        uint256 length = ids.length;
        for (uint256 i; i < length; ) {
            delete inflightMsgs[ids[i]];
            unchecked {
                ++i;
            }
        }
    }

    function _onlyGateway() private view {
        // only the gateway address is allowed to deliver xnet messages.
        if (msg.sender != gatewayAddr) {
            revert IIpcHandler.CallerIsNotGateway();
        }
    }

    modifier onlyGateway() {
        _onlyGateway();
        _;
    }
}
