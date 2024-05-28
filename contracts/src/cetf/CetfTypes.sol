// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "filecoin-solidity-api/types/CommonTypes.sol";
import "filecoin-solidity-api/cbor/BytesCbor.sol";

/// @title CETF actor types for Solidity.
/// @author BadBoi Labs
library CetfTypes {
    using BytesCBOR for bytes;

    CommonTypes.FilActorId constant ActorID = CommonTypes.FilActorId.wrap(49);
    uint256 constant EnqueueTagMethodNum = 18206847610; // obtained from actors/cetf/actor.rs  actor_dispatch! macro

    struct EnqueueTagParams {
        bytes32 tag;
    }

    function serializeEnqueueTagParams(EnqueueTagParams memory params) internal pure returns (bytes memory) {
        bytes memory b = abi.encodePacked(params.tag);
        return b.serializeBytes();
    }
}
