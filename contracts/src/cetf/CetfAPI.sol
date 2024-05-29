// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "filecoin-solidity-api/types/CommonTypes.sol";
import "filecoin-solidity-api/utils/Misc.sol";
import "filecoin-solidity-api/utils/Actor.sol";
import "solidity-cborutils/contracts/CBOR.sol";


/// @title This library is a proxy to the singleton CETF actor (address: f05)
/// @author BadBoi Labs
library CetfAPI {
    using CBOR for CBOR.CBORBuffer;

    CommonTypes.FilActorId constant ActorID = CommonTypes.FilActorId.wrap(49);
    uint256 constant EchoMethodNum = 1638589290; // obtained from actors/cetf/actor.rs  actor_dispatch! macro
    uint256 constant EnqueueTagMethodNum = 1820684761; // obtained from actors/cetf/actor.rs  actor_dispatch! macro


    function serializeEnqueueTagParams(bytes32 tag) internal pure returns (bytes memory) {
        // Create a bytes array with length 32
        bytes memory bytesTag = new bytes(32);
        
        // Copy each byte from bytes32 to the bytes array
        for (uint i = 0; i < 32; i++) {
            bytesTag[i] = tag[i];
        }

        uint256 capacity = 0;

        capacity += Misc.getPrefixSize(1);
        capacity += Misc.getBytesSize(bytesTag);

        CBOR.CBORBuffer memory buf = CBOR.create(capacity);

        buf.startFixedArray(1);
        buf.writeBytes(bytesTag);

        return buf.data();
    }

    function serializeEchoParams() internal pure returns (bytes memory) {
        CBOR.CBORBuffer memory buf = CBOR.create(1);
        buf.writeUndefined();
        return buf.data();
    }

    function echo() internal returns (int256) {
        bytes memory rawParams = serializeEchoParams();

        (int256 exitCode,) =
            Actor.callByID(ActorID, EchoMethodNum, Misc.CBOR_CODEC, rawParams, 0, false);
        return (exitCode);
    }

    function enqueueTag(bytes32 tag) internal returns (int256) {
        bytes memory rawParams = serializeEnqueueTagParams(tag);

        (int256 exitCode,) =
            Actor.callByID(ActorID, EnqueueTagMethodNum, Misc.CBOR_CODEC, rawParams, 0, false);
        return (exitCode);
    }
}
