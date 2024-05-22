// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {LibGateway} from "../../src/lib/LibGateway.sol";
import {IpcEnvelope, BottomUpMsgBatch} from "../../src/structs/CrossNet.sol";
import {SubnetID} from "../../src/structs/Subnet.sol";
import {GatewayActorStorage, LibGatewayActorStorage} from "../../src/lib/LibGatewayActorStorage.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";

contract LibGatewayMock {
    using SubnetIDHelper for SubnetID;

    /// Util function to set the current subnet network name
    function setSubnet(SubnetID memory subnet, uint256 bottomUpPeriod) public {
        GatewayActorStorage storage s = LibGatewayActorStorage.appStorage();
        s.networkName = subnet;
        s.bottomUpCheckPeriod = bottomUpPeriod;

        s.subnets[subnet.toHash()].id = subnet;
    }

    function registerSubnet(SubnetID memory subnet) public {
        GatewayActorStorage storage s = LibGatewayActorStorage.appStorage();
        s.subnets[subnet.toHash()].id = subnet;
    }

    function getNextBottomUpMsgBatch() public view returns (BottomUpMsgBatch memory batch) {
        GatewayActorStorage storage s = LibGatewayActorStorage.appStorage();
        uint256 epoch = LibGateway.getNextEpoch(block.number, s.bottomUpCheckPeriod);

        batch = s.bottomUpMsgBatches[epoch];
    }

    function applyMsg(SubnetID memory arrivingFrom, IpcEnvelope memory crossMsg) public {
        LibGateway.applyMsg(arrivingFrom, crossMsg);
    }
}
