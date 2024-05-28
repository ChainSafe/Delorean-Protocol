// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "./CetfTypes.sol";

import "filecoin-solidity-api/types/CommonTypes.sol";
import "filecoin-solidity-api/utils/Misc.sol";
import "filecoin-solidity-api/utils/Actor.sol";

/// @title This library is a proxy to the singleton CETF actor (address: f05)
/// @author BadBoi Labs
library CetfAPI {
    using CetfTypes for *;

    function enqueueTag(bytes32 tag) internal returns (int256) {
        CetfTypes.EnqueueTagParams memory params = CetfTypes.EnqueueTagParams(tag);
        bytes memory rawRequest = params.serializeEnqueueTagParams();

        (int256 exitCode,) =
            Actor.callByID(CetfTypes.ActorID, CetfTypes.EnqueueTagMethodNum, Misc.CBOR_CODEC, rawRequest, 0, false);
        return (exitCode);
    }
}
