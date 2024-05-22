// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "forge-std/console.sol";

import "../../src/errors/IPCErrors.sol";
import {IpcEnvelope, CallMsg, ResultMsg, IpcMsgKind, OutcomeType} from "../../src/structs/CrossNet.sol";
import {FvmAddress} from "../../src/structs/FvmAddress.sol";
import {SubnetID, Subnet, IPCAddress, Validator} from "../../src/structs/Subnet.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";
import {FvmAddressHelper} from "../../src/lib/FvmAddressHelper.sol";
import {CrossMsgHelper} from "../../src/lib/CrossMsgHelper.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {IpcExchange} from "../../sdk/IpcContract.sol";
import {IIpcHandler} from "../../sdk/interfaces/IIpcHandler.sol";
import {IGateway} from "../../src/interfaces/IGateway.sol";
import {CrossMsgHelper} from "../../src/lib/CrossMsgHelper.sol";

interface Foo {
    function foo(string calldata) external returns (string memory);
}

contract RecorderIpcExchange is IpcExchange {
    IpcEnvelope private lastEnvelope;
    CallMsg private lastCallMsg;
    ResultMsg private lastResultMsg;
    bool private shouldRevert;

    constructor(address gatewayAddr_) IpcExchange(gatewayAddr_) {}

    function _handleIpcCall(
        IpcEnvelope memory envelope,
        CallMsg memory callMsg
    ) internal override returns (bytes memory) {
        require(!shouldRevert, "revert requested");
        console.log("handling ipc call");
        lastEnvelope = envelope;
        lastCallMsg = callMsg;
        return bytes("");
    }

    function _handleIpcResult(
        IpcEnvelope storage,
        IpcEnvelope memory result,
        ResultMsg memory resultMsg
    ) internal override {
        require(!shouldRevert, "revert requested");
        console.log("handling ipc result");
        lastEnvelope = result;
        lastResultMsg = resultMsg;
    }

    function flipRevert() public {
        shouldRevert = !shouldRevert;
    }

    // Expose this method so we can test it.
    function performIpcCall_(IPCAddress calldata to, CallMsg calldata callMsg, uint256 value) public {
        performIpcCall(to, callMsg, value);
    }

    // We need these manual getters because Solidity-generated ones on public fields decompose the struct
    // into its constituents.
    function getLastEnvelope() public view returns (IpcEnvelope memory) {
        return lastEnvelope;
    }

    // We need these manual getters because Solidity-generated ones on public fields decompose the struct
    // into its constituents.
    function getLastCallMsg() public view returns (CallMsg memory) {
        return lastCallMsg;
    }

    // We need these manual getters because Solidity-generated ones on public fields decompose the struct
    // into its constituents.
    function getLastResultMsg() public view returns (ResultMsg memory) {
        return lastResultMsg;
    }

    // We need these manual getters because Solidity-generated ones on public fields decompose the struct
    // into its constituents.
    function getInflight(bytes32 id) public view returns (IpcEnvelope memory) {
        return inflightMsgs[id];
    }
}

contract IpcExchangeTest is Test {
    using CrossMsgHelper for IpcEnvelope;
    address gateway = vm.addr(1);
    SubnetID subnetA;
    SubnetID subnetB;
    CallMsg callMsg;
    ResultMsg resultMsg;
    IpcEnvelope callEnvelope;
    IpcEnvelope resultEnvelope;
    RecorderIpcExchange exch;

    IPCAddress ipcAddressA;
    IPCAddress ipcAddressB;

    function setUp() public {
        address[] memory pathA = new address[](1);
        pathA[0] = vm.addr(2000);
        address[] memory pathB = new address[](1);
        pathB[0] = vm.addr(3000);

        // these two subnets are siblings.
        subnetA = SubnetID({root: 123, route: pathA});
        subnetB = SubnetID({root: 123, route: pathB});
        ipcAddressA = IPCAddress({subnetId: subnetA, rawAddress: FvmAddressHelper.from(address(100))});
        ipcAddressB = IPCAddress({subnetId: subnetB, rawAddress: FvmAddressHelper.from(address(200))});

        callMsg = CallMsg({method: abi.encodePacked(Foo.foo.selector), params: bytes("1234")});
        callEnvelope = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: ipcAddressA,
            to: ipcAddressB,
            value: 1000,
            message: abi.encode(callMsg),
            nonce: 0
        });

        resultMsg = ResultMsg({outcome: OutcomeType.Ok, id: callEnvelope.toHash(), ret: bytes("")});

        resultEnvelope = IpcEnvelope({
            kind: IpcMsgKind.Result,
            from: ipcAddressB,
            to: ipcAddressA,
            value: 1000,
            message: abi.encode(resultMsg),
            nonce: 0
        });

        exch = new RecorderIpcExchange(gateway);
    }

    function test_IpcExchange_testTransferFails() public {
        callEnvelope.kind = IpcMsgKind.Transfer;

        // a transfer; fails because cannot handle.
        vm.expectRevert(IIpcHandler.UnsupportedMsgKind.selector);
        vm.prank(gateway);
        exch.handleIpcMessage(callEnvelope);
    }

    function test_IpcExchange_testGatewayOnlyFails() public {
        // a call; fails when the caller is not the gateway.
        vm.expectRevert(IIpcHandler.CallerIsNotGateway.selector);
        exch.handleIpcMessage(callEnvelope);
    }

    function test_IpcExchange_handleOk() public {
        vm.startPrank(gateway);
        exch.handleIpcMessage(callEnvelope);

        // succeeds.
        IpcEnvelope memory lastEnvelope = exch.getLastEnvelope();
        CallMsg memory lastCall = exch.getLastCallMsg();
        require(keccak256(abi.encode(callEnvelope)) == keccak256(abi.encode(lastEnvelope)), "unexpected callEnvelope");
        require(keccak256(abi.encode(callMsg)) == keccak256(abi.encode(lastCall)), "unexpected callmsg");
    }

    function test_IpcExchange_revertPropagated() public {
        vm.startPrank(gateway);
        // a revert bubbles up.
        exch.flipRevert();
        vm.expectRevert("revert requested");
        exch.handleIpcMessage(callEnvelope);
    }

    function test_IpcExchange_unexpectedResult() public {
        vm.startPrank(gateway);

        // an unrecognized result
        callEnvelope.kind = IpcMsgKind.Result;
        callEnvelope.message = abi.encode(ResultMsg({outcome: OutcomeType.Ok, id: keccak256("foo"), ret: bytes("")}));

        IPCAddress memory from = callEnvelope.from;
        callEnvelope.from = callEnvelope.to;
        callEnvelope.to = from;

        vm.expectRevert(IIpcHandler.UnrecognizedResult.selector);
        exch.handleIpcMessage(callEnvelope);
    }

    function test_IpcExchange_successfulCorrelation() public {
        // Perform an outgoing IPC call from within the contract.
        vm.mockCall(
            gateway,
            abi.encodeWithSelector(IGateway.sendContractXnetMessage.selector),
            abi.encode(callEnvelope)
        );
        vm.deal(address(this), 1000);
        exch.performIpcCall_(ipcAddressA, callMsg, 1);
        // assert that we stored the correct callEnvelope in the correlation map.
        IpcEnvelope memory correlated = exch.getInflight(callEnvelope.toHash());
        require(correlated.toHash() == callEnvelope.toHash());

        vm.startPrank(gateway);

        // Simulate an OK incoming result.
        exch.handleIpcMessage(resultEnvelope);
        require(exch.getLastResultMsg().id != bytes32(""), "_handleIpcResult was not called");
    }

    function test_IpcExchange_dropMessages() public {
        vm.deal(address(this), 1000);

        // Send three messages from within the contract.
        bytes32[] memory ids = new bytes32[](3);
        for (uint64 i = 0; i < 3; i++) {
            callEnvelope.nonce = i;
            vm.mockCall(
                gateway,
                abi.encodeWithSelector(IGateway.sendContractXnetMessage.selector),
                abi.encode(callEnvelope)
            );
            exch.performIpcCall_(ipcAddressA, callMsg, 1);

            bytes32 id = callEnvelope.toHash();
            require(exch.getInflight(id).value != 0, "envelope not found in correlation map");

            ids[i] = id;
        }

        bytes32[] memory params = new bytes32[](2);
        params[0] = ids[0];
        params[1] = ids[1];

        // drop a message: unauthorized
        vm.prank(address(vm.addr(999)));
        vm.expectRevert();
        exch.dropMessages(params);

        // drop a message: from the owner
        vm.prank(address(this));
        exch.dropMessages(params);

        require(exch.getInflight(ids[0]).value == 0, "did not expect envelope in correlation map");
        require(exch.getInflight(ids[1]).value == 0, "did not expect envelope in correlation map");
        require(exch.getInflight(ids[2]).value != 0, "expected envelope in correlation map");

        vm.startPrank(gateway);

        // unrecognized correlation id
        vm.expectRevert(IIpcHandler.UnrecognizedResult.selector);
        exch.handleIpcMessage(resultEnvelope);

        // only remaining one
        resultMsg.id = ids[2];
        resultEnvelope.message = abi.encode(resultMsg);

        // assert that we stored the correct callEnvelope in the correlation map.
        IpcEnvelope memory correlated = exch.getInflight(callEnvelope.toHash());
        require(correlated.toHash() == callEnvelope.toHash());

        // Simulate an OK incoming result.
        exch.handleIpcMessage(resultEnvelope);
        require(exch.getLastResultMsg().id != bytes32(""), "_handleIpcResult was not called");
    }
}
