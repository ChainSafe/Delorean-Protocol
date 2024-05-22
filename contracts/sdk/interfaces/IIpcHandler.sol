// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {IpcEnvelope, ResultMsg, CallMsg, IpcMsgKind} from "../../src/structs/CrossNet.sol";

// Interface that needs to be implemented by IPC-aware contracts.
interface IIpcHandler {
    error CallerIsNotGateway();
    error UnsupportedMsgKind();
    error UnrecognizedResult();

    /// @notice Entrypoint for handling xnet messages in IPC-aware contracts.
    function handleIpcMessage(IpcEnvelope calldata envelope) external payable returns (bytes memory ret);
}
