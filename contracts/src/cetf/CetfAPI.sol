// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import "filecoin-solidity-api/types/CommonTypes.sol";
import "filecoin-solidity-api/utils/Misc.sol";
import "filecoin-solidity-api/utils/Actor.sol";
import "solidity-cborutils/contracts/CBOR.sol";


/// @title This library is a proxy to the singleton CETF actor (address: f05)
/// @author BadBoi Labs
library CetfAPI {
    CommonTypes.FilActorId constant ActorID = CommonTypes.FilActorId.wrap(49);
    uint256 constant EnqueueTagMethodNum = 18206847610; // obtained from actors/cetf/actor.rs  actor_dispatch! macro

    struct EnqueueTagParams {
        bytes tag;
    }

    /// @notice serialize WithdrawBalanceParams struct to cbor in order to pass as arguments to the market actor
    /// @param params WithdrawBalanceParams to serialize as cbor
    /// @return response cbor serialized data as bytes
    function serializeEnqueueTagParams(EnqueueTagParams memory params) internal pure returns (bytes memory) {
        uint256 capacity = 0;
        bytes memory tokenAmount = params.tokenAmount.serializeBigInt();

        capacity += Misc.getPrefixSize(2);
        capacity += Misc.getBytesSize(params.provider_or_client.data);
        capacity += Misc.getBytesSize(tokenAmount);
        CBOR.CBORBuffer memory buf = CBOR.create(capacity);

        buf.startFixedArray(2);
        buf.writeBytes(params.provider_or_client.data);
        buf.writeBytes(tokenAmount);

        return buf.data();
    }

    function enqueueTag(bytes32 tag) internal returns (int256) {
        bytes memory rawParams = abi.encodePacked(tag);

        (int256 exitCode,) =
            Actor.callByID(ActorID, EnqueueTagMethodNum, Misc.CBOR_CODEC, rawParams, 0, false);
        return (exitCode);
    }
}
