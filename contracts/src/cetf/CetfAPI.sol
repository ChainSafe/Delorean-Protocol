// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "filecoin-solidity-api/types/CommonTypes.sol";
import "filecoin-solidity-api/utils/Misc.sol";
import "filecoin-solidity-api/utils/Actor.sol";

/// @title This library is a proxy to the singleton CETF actor (address: f05)
/// @author BadBoi Labs
library CetfAPI {
    CommonTypes.FilActorId constant ActorID = CommonTypes.FilActorId.wrap(49);
    uint256 constant EnqueueTagMethodNum = 1820684761; // obtained from actors/cetf/actor.rs  actor_dispatch! macro

    function enqueueTag(bytes32 tag) internal returns (int256) {
        bytes memory rawParams = abi.encodePacked(tag);

        (int256 exitCode,) =
            Actor.callByID(ActorID, EnqueueTagMethodNum, Misc.CBOR_CODEC, rawParams, 0, false);
        return (exitCode);
    }
}
