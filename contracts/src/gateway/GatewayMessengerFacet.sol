// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {GatewayActorModifiers} from "../lib/LibGatewayActorStorage.sol";
import {IpcEnvelope, CallMsg, IpcMsgKind} from "../structs/CrossNet.sol";
import {IPCMsgType} from "../enums/IPCMsgType.sol";
import {SubnetID, SupplyKind, IPCAddress} from "../structs/Subnet.sol";
import {InvalidXnetMessage, InvalidXnetMessageReason, CannotSendCrossMsgToItself, MethodNotAllowed} from "../errors/IPCErrors.sol";
import {SubnetIDHelper} from "../lib/SubnetIDHelper.sol";
import {LibGateway} from "../lib/LibGateway.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {SupplySourceHelper} from "../lib/SupplySourceHelper.sol";
import {CrossMsgHelper} from "../lib/CrossMsgHelper.sol";
import {FvmAddressHelper} from "../lib/FvmAddressHelper.sol";

string constant ERR_GENERAL_CROSS_MSG_DISABLED = "Support for general-purpose cross-net messages is disabled";
string constant ERR_MULTILEVEL_CROSS_MSG_DISABLED = "Support for multi-level cross-net messages is disabled";

contract GatewayMessengerFacet is GatewayActorModifiers {
    using FilAddress for address payable;
    using SubnetIDHelper for SubnetID;

    /**
     * @dev Sends a general-purpose cross-message from the local subnet to the destination subnet.
     * Any value in msg.value will be forwarded in the call.
     *
     * IMPORTANT: Only smart contracts are allowed to trigger these cross-net messages. User wallets can send funds
     * from their address to the destination subnet and then run the transaction in the destination normally.
     *
     * @param envelope - the original envelope, which will be validated, stamped and committed during the send.
     * @return committed envelope.
     */
    function sendContractXnetMessage(
        IpcEnvelope calldata envelope
    ) external payable returns (IpcEnvelope memory committed) {
        if (!s.generalPurposeCrossMsg) {
            revert MethodNotAllowed(ERR_GENERAL_CROSS_MSG_DISABLED);
        }

        // We prevent the sender from being an EoA.
        if (!(msg.sender.code.length > 0)) {
            revert InvalidXnetMessage(InvalidXnetMessageReason.Sender);
        }

        if (envelope.value != msg.value) {
            revert InvalidXnetMessage(InvalidXnetMessageReason.Value);
        }

        if (envelope.kind != IpcMsgKind.Call) {
            revert InvalidXnetMessage(InvalidXnetMessageReason.Kind);
        }

        // Will revert if the message won't deserialize into a CallMsg.
        abi.decode(envelope.message, (CallMsg));

        committed = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: s.networkName, rawAddress: FvmAddressHelper.from(msg.sender)}),
            to: envelope.to,
            value: msg.value,
            message: envelope.message,
            nonce: 0 // nonce will be updated by LibGateway.commitCrossMessage
        });

        // Commit xnet message for dispatch.
        bool shouldBurn = LibGateway.commitCrossMessage(committed);

        // Apply side effects, such as burning funds.
        LibGateway.crossMsgSideEffects({v: committed.value, shouldBurn: shouldBurn});

        // Return a copy of the envelope, which was updated when it was committed.
        // Updates are visible to us because commitCrossMessage takes the envelope with memory scope,
        // which passes the struct by reference.
        return committed;
    }

    /**
     * @dev propagates the populated cross net message for the given cid
     * @param msgCid - the cid of the cross-net message
     */
    function propagate(bytes32 msgCid) external payable {
        if (!s.multiLevelCrossMsg) {
            revert MethodNotAllowed(ERR_MULTILEVEL_CROSS_MSG_DISABLED);
        }

        IpcEnvelope storage crossMsg = s.postbox[msgCid];

        bool shouldBurn = LibGateway.commitCrossMessage(crossMsg);
        // We must delete the message first to prevent potential re-entrancies,
        // and as the message is deleted and we don't have a reference to the object
        // anymore, we need to pull the data from the message to trigger the side-effects.
        uint256 v = crossMsg.value;
        delete s.postbox[msgCid];

        LibGateway.crossMsgSideEffects({v: v, shouldBurn: shouldBurn});
    }
}
