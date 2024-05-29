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


    function serializeEnqueueTagParams(uint64 tag) internal pure returns (bytes memory) {
        CBOR.CBORBuffer memory buf = CBOR.create(0);
        buf.writeUInt64(tag);
        return buf.data();
    }

    function serializeEchoParams() internal pure returns (bytes memory) {
        CBOR.CBORBuffer memory buf = CBOR.create(0);
        buf.writeNull();
        return buf.data();
    }

    function echo() internal returns (int256) {
        bytes memory rawParams = serializeEchoParams();

        (int256 exitCode,) =
            Actor.callByID(ActorID, EchoMethodNum, Misc.CBOR_CODEC, rawParams, 0, false);
        return (exitCode);
    }

    function enqueueTag(uint64 tag) internal returns (int256) {
        bytes memory rawParams = serializeEnqueueTagParams(tag);

        (int256 exitCode,) =
            Actor.callByID(ActorID, EnqueueTagMethodNum, Misc.CBOR_CODEC, rawParams, 0, false);
        return (exitCode);
    }
}
