// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";

import {LibGatewayMock} from "../mocks/LibGatewayMock.sol";
import {LibGateway} from "../../src/lib/LibGateway.sol";
import {IpcEnvelope, IpcMsgKind, ResultMsg, OutcomeType, BottomUpMsgBatch} from "../../src/structs/CrossNet.sol";
import {SubnetID, IPCAddress} from "../../src/structs/Subnet.sol";
import {FvmAddressHelper} from "../../src/lib/FvmAddressHelper.sol";
import {CrossMsgHelper} from "../../src/lib/CrossMsgHelper.sol";
import {SubnetActorGetterFacet} from "../../src/subnet/SubnetActorGetterFacet.sol";
import {InvalidXnetMessage, InvalidXnetMessageReason} from "../../src/errors/IPCErrors.sol";
import {MockIpcContract} from "../helpers/TestUtils.sol";
import {EMPTY_BYTES} from "../../src/constants/Constants.sol";

contract GatewayDummyContract {
    function reverts() public pure {
        require(false, "hey, revert here");
    }
}

contract LibGatewayTest is Test {
    using CrossMsgHelper for IpcEnvelope;

    function test_applyMsg_receiptFailure() public {
        LibGatewayMock t = new LibGatewayMock();

        SubnetID memory subnetId = SubnetID({root: 0, route: new address[](0)});
        t.setSubnet(subnetId, 1);

        // This message carries an empty subnet, which will make the gateway want to return an error
        // result. However, the message payload is empty and will fail the isEmpty() check, so we'll also
        // skip sending a receipt.
        IpcEnvelope memory envelope = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: subnetId, rawAddress: FvmAddressHelper.from(address(1))}),
            to: IPCAddress({subnetId: subnetId, rawAddress: FvmAddressHelper.from(address(2))}),
            value: 0,
            message: new bytes(0),
            nonce: 0
        });

        vm.recordLogs();

        t.applyMsg(subnetId, envelope);

        // no bottom up nor top down interactions; caveat: not checking the postbox.
        require(vm.getRecordedLogs().length == 0, "did not expect events");
        require(t.getNextBottomUpMsgBatch().msgs.length == 0, "did not expect bottup up messages");
    }

    function test_applyMsg_transferNoOpt() public {
        LibGatewayMock t = new LibGatewayMock();
        SubnetID memory subnetId = SubnetID({root: 1, route: new address[](0)});

        IpcEnvelope memory envelope = IpcEnvelope({
            kind: IpcMsgKind.Transfer,
            from: IPCAddress({subnetId: subnetId, rawAddress: FvmAddressHelper.from(address(1))}),
            to: IPCAddress({subnetId: subnetId, rawAddress: FvmAddressHelper.from(address(2))}),
            value: 0,
            message: new bytes(0),
            nonce: 0
        });

        t.applyMsg(subnetId, envelope);
    }

    function test_applyMsg_bottomUpSuccess() public {
        LibGatewayMock t = new LibGatewayMock();

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(parentSubnet, 1);
        t.registerSubnet(childSubnet);

        address fromRaw = address(1000);
        address toRaw = address(new MockIpcContract());

        IPCAddress memory from = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        vm.deal(address(t), 1000);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 1000,
            method: GatewayDummyContract.reverts.selector,
            params: new bytes(0)
        });
        crossMsg.nonce = 0;

        ResultMsg memory message = ResultMsg({
            outcome: OutcomeType.Ok,
            id: crossMsg.toHash(),
            ret: abi.encode(EMPTY_BYTES)
        });
        IpcEnvelope memory expected = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: crossMsg.to,
            to: crossMsg.from,
            value: 0, // it succeeded
            message: abi.encode(message),
            nonce: 0
        });

        vm.expectEmit(address(t));
        emit LibGateway.NewTopDownMessage(childSubnetActor, expected);

        t.applyMsg(childSubnet, crossMsg);
    }

    function test_applyMsg_topDownSuccess() public {
        LibGatewayMock t = new LibGatewayMock();

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(childSubnet, 1);

        address fromRaw = address(1000);
        // use mock IpcContract so it succeeds.
        address toRaw = address(new MockIpcContract());

        IPCAddress memory from = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        vm.deal(address(t), 1000);
        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 1000,
            method: bytes4(0),
            params: new bytes(0)
        });
        crossMsg.nonce = 0;

        ResultMsg memory message = ResultMsg({
            outcome: OutcomeType.Ok,
            id: crossMsg.toHash(),
            ret: abi.encode(EMPTY_BYTES)
        });
        IpcEnvelope memory expected = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: crossMsg.to,
            to: crossMsg.from,
            value: 0, // it succeeded
            message: abi.encode(message),
            nonce: 0
        });

        t.applyMsg(parentSubnet, crossMsg);

        BottomUpMsgBatch memory batch = t.getNextBottomUpMsgBatch();
        require(batch.msgs.length == 1, "should have bottom up messages");
        IpcEnvelope memory stored = batch.msgs[0];
        require(keccak256(stored.message) == keccak256(expected.message), "receipt message not matching");
        require(stored.toHash() == expected.toHash(), "receipt hash not matching");
    }

    function test_applyMsg_topdownInvalidNonce() public {
        LibGatewayMock t = new LibGatewayMock();

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(childSubnet, 1);

        address fromRaw = address(1000);
        address toRaw = address(1001);

        IPCAddress memory from = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 1000,
            method: bytes4(0),
            params: new bytes(0)
        });
        crossMsg.nonce = 10; // a wrong nonce

        ResultMsg memory message = ResultMsg({
            outcome: OutcomeType.SystemErr,
            id: crossMsg.toHash(),
            ret: abi.encodeWithSelector(InvalidXnetMessage.selector, InvalidXnetMessageReason.Nonce)
        });
        IpcEnvelope memory expected = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: crossMsg.to,
            to: crossMsg.from,
            value: crossMsg.value,
            message: abi.encode(message),
            nonce: 0
        });

        t.applyMsg(parentSubnet, crossMsg);

        BottomUpMsgBatch memory batch = t.getNextBottomUpMsgBatch();
        require(batch.msgs.length == 1, "should have bottom up messages");
        IpcEnvelope memory stored = batch.msgs[0];
        require(stored.toHash() == expected.toHash(), "receipt hash not matching");
    }

    function test_applyMsg_topdownReverts() public {
        LibGatewayMock t = new LibGatewayMock();

        address callingContract = address(new GatewayDummyContract());

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(childSubnet, 1);

        address fromRaw = address(1000);
        address toRaw = callingContract;

        IPCAddress memory from = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 0,
            method: GatewayDummyContract.reverts.selector,
            params: new bytes(0)
        });
        crossMsg.nonce = 0;

        ResultMsg memory message = ResultMsg({outcome: OutcomeType.ActorErr, id: crossMsg.toHash(), ret: new bytes(0)});
        IpcEnvelope memory expected = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: crossMsg.to,
            to: crossMsg.from,
            value: 0,
            message: abi.encode(message),
            nonce: 0
        });

        t.applyMsg(parentSubnet, crossMsg);

        BottomUpMsgBatch memory batch = t.getNextBottomUpMsgBatch();
        require(batch.msgs.length == 1, "should have bottom up messages");
        IpcEnvelope memory stored = batch.msgs[0];

        require(stored.toHash() == expected.toHash(), "receipt hash not matching");
    }

    function test_applyMsg_bottomUpNotRegistered() public {
        LibGatewayMock t = new LibGatewayMock();

        address callingContract = address(new GatewayDummyContract());

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(parentSubnet, 1);

        address fromRaw = address(1000);
        address toRaw = callingContract;

        IPCAddress memory from = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 0,
            method: GatewayDummyContract.reverts.selector,
            params: new bytes(0)
        });
        crossMsg.nonce = 0;

        t.applyMsg(childSubnet, crossMsg);
    }

    function test_applyMsg_bottomUpInvalidNonce() public {
        LibGatewayMock t = new LibGatewayMock();
        address callingContract = address(new GatewayDummyContract());

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(parentSubnet, 1);
        t.registerSubnet(childSubnet);

        address fromRaw = address(1000);
        address toRaw = callingContract;

        IPCAddress memory from = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 1000,
            method: GatewayDummyContract.reverts.selector,
            params: new bytes(0)
        });
        crossMsg.nonce = 10;

        ResultMsg memory message = ResultMsg({
            outcome: OutcomeType.SystemErr,
            id: crossMsg.toHash(),
            ret: abi.encodeWithSelector(InvalidXnetMessage.selector, InvalidXnetMessageReason.Nonce)
        });
        IpcEnvelope memory expected = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: crossMsg.to,
            to: crossMsg.from,
            value: crossMsg.value,
            message: abi.encode(message),
            nonce: 0
        });

        vm.expectEmit(address(t));
        emit LibGateway.NewTopDownMessage(childSubnetActor, expected);

        t.applyMsg(childSubnet, crossMsg);
    }

    function test_applyMsg_bottomUpExecutionFails() public {
        LibGatewayMock t = new LibGatewayMock();
        address callingContract = address(new GatewayDummyContract());

        address childSubnetActor = address(new SubnetActorGetterFacet());

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(1);

        address[] memory childRoute = new address[](2);
        childRoute[0] = address(1);
        childRoute[1] = childSubnetActor;

        SubnetID memory parentSubnet = SubnetID({root: 1, route: parentRoute});
        SubnetID memory childSubnet = SubnetID({root: 1, route: childRoute});

        t.setSubnet(parentSubnet, 1);
        t.registerSubnet(childSubnet);

        address fromRaw = address(1000);
        address toRaw = callingContract;

        IPCAddress memory from = IPCAddress({subnetId: childSubnet, rawAddress: FvmAddressHelper.from(fromRaw)});
        IPCAddress memory to = IPCAddress({subnetId: parentSubnet, rawAddress: FvmAddressHelper.from(toRaw)});

        IpcEnvelope memory crossMsg = CrossMsgHelper.createCallMsg({
            from: from,
            to: to,
            value: 1000,
            method: GatewayDummyContract.reverts.selector,
            params: new bytes(0)
        });
        crossMsg.nonce = 0;

        ResultMsg memory message = ResultMsg({outcome: OutcomeType.ActorErr, id: crossMsg.toHash(), ret: new bytes(0)});
        IpcEnvelope memory expected = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: crossMsg.to,
            to: crossMsg.from,
            value: crossMsg.value,
            message: abi.encode(message),
            nonce: 0
        });

        vm.deal(address(t), 1 ether);
        vm.expectEmit(address(t));
        emit LibGateway.NewTopDownMessage(childSubnetActor, expected);

        t.applyMsg(childSubnet, crossMsg);
    }

    function test_nextCheckpointEpoch() public pure {
        uint64 checkpointPeriod = 10;

        require(LibGateway.getNextEpoch(0, checkpointPeriod) == checkpointPeriod, "next epoch not correct");
        require(LibGateway.getNextEpoch(1, checkpointPeriod) == checkpointPeriod, "next epoch not correct");
        require(LibGateway.getNextEpoch(10, checkpointPeriod) == checkpointPeriod * 2, "next epoch not correct");
        require(LibGateway.getNextEpoch(15, checkpointPeriod) == checkpointPeriod * 2, "next epoch not correct");

        checkpointPeriod = 17;

        require(LibGateway.getNextEpoch(0, checkpointPeriod) == checkpointPeriod, "next epoch not correct");
        require(
            LibGateway.getNextEpoch(checkpointPeriod - 1, checkpointPeriod) == checkpointPeriod,
            "next epoch not correct"
        );
        require(
            LibGateway.getNextEpoch(checkpointPeriod, checkpointPeriod) == checkpointPeriod * 2,
            "next epoch not correct"
        );
        require(
            LibGateway.getNextEpoch(checkpointPeriod + 1, checkpointPeriod) == checkpointPeriod * 2,
            "next epoch not correct"
        );
    }
}
