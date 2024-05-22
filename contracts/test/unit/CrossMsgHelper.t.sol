// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "../../src/lib/CrossMsgHelper.sol";
import "../../src/lib/SubnetIDHelper.sol";
import "../../src/lib/FvmAddressHelper.sol";
import {FvmAddress} from "../../src/structs/FvmAddress.sol";
import {SupplySource} from "../../src/structs/Subnet.sol";
import {IpcMsgKind, CallMsg} from "../../src/structs/CrossNet.sol";

import "openzeppelin-contracts/utils/Address.sol";

contract CrossMsgHelperTest is Test {
    using SubnetIDHelper for SubnetID;
    using CrossMsgHelper for IpcEnvelope;
    using CrossMsgHelper for IpcEnvelope[];
    using CallMsgTestHelper for IpcEnvelope;
    using FvmAddressHelper for FvmAddress;

    uint64 private constant ROOTNET_CHAINID = 123;
    uint256 CROSS_MESSAGE_FEE = 1 gwei;

    IpcEnvelope public crossMsg;
    IpcEnvelope[] public crossMsgs;

    error NoParentForSubnet();

    function test_IsEmpty_Works_EmptyCrossMsg() public view {
        require(crossMsg.isEmpty() == true);
    }

    function test_ToHash_Works() public view {
        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = crossMsg;
        require(CrossMsgHelper.toHash(crossMsg) == CrossMsgHelper.toHash(msgs[0]));
        require(CrossMsgHelper.toHash(crossMsg) != CrossMsgHelper.toHash(msgs));
    }

    function test_IsEmpty_Works_NonEmptyCrossMsg() public {
        crossMsg.kind = IpcMsgKind.Call;
        crossMsg.message = bytes("hello");

        require(crossMsg.isEmpty() == false);
    }

    function test_CreateReleaseMsg_Works(uint256 releaseAmount, address sender) public {
        address[] memory route = new address[](2);
        route[0] = makeAddr("root");
        route[1] = makeAddr("subnet");
        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, route);

        vm.prank(sender);

        IpcEnvelope memory releaseMsg = CrossMsgHelper.createReleaseMsg(
            subnetId,
            sender,
            FvmAddressHelper.from(sender),
            releaseAmount
        );

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = route[0];
        SubnetID memory parentSubnetId = SubnetID(ROOTNET_CHAINID, parentRoute);

        require(releaseMsg.from.subnetId.toHash() == subnetId.toHash());
        require(releaseMsg.from.rawAddress.extractEvmAddress() == sender);
        require(releaseMsg.to.subnetId.toHash() == parentSubnetId.toHash());
        require(releaseMsg.to.rawAddress.extractEvmAddress() == sender);
        require(releaseMsg.value == releaseAmount);
        require(releaseMsg.nonce == 0);
        require(releaseMsg.message.length == 0);
        require(releaseMsg.kind == IpcMsgKind.Transfer);
    }

    function test_CreateReleaseMsg_Fails_SubnetNoParent(uint256 releaseAmount, address sender) public {
        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, new address[](0));

        vm.expectRevert(NoParentForSubnet.selector);

        CrossMsgHelper.createReleaseMsg(subnetId, sender, FvmAddressHelper.from(sender), releaseAmount);
    }

    function test_CreateFundMsg_Works_Root(uint256 fundAmount, address sender) public {
        address[] memory parentRoute = new address[](1);
        parentRoute[0] = address(101);
        SubnetID memory parentSubnetId = SubnetID(ROOTNET_CHAINID, parentRoute);

        vm.prank(sender);

        IpcEnvelope memory fundMsg = CrossMsgHelper.createFundMsg(
            parentSubnetId,
            sender,
            FvmAddressHelper.from(sender),
            fundAmount
        );

        SubnetID memory rootSubnetId = SubnetID(ROOTNET_CHAINID, new address[](0));

        require(fundMsg.from.subnetId.toHash() == rootSubnetId.toHash());
        require(fundMsg.from.rawAddress.extractEvmAddress() == sender);
        require(fundMsg.to.subnetId.toHash() == parentSubnetId.toHash());
        require(fundMsg.to.rawAddress.extractEvmAddress() == sender);
        require(fundMsg.value == fundAmount);
        require(fundMsg.nonce == 0);
        require(fundMsg.message.length == 0);
        require(fundMsg.kind == IpcMsgKind.Transfer);
    }

    function test_CreateFundMsg_Works(uint256 fundAmount, address sender) public {
        address[] memory route = new address[](2);
        route[0] = makeAddr("root");
        route[1] = makeAddr("subnet");
        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, route);

        vm.prank(sender);

        IpcEnvelope memory fundMsg = CrossMsgHelper.createFundMsg(
            subnetId,
            sender,
            FvmAddressHelper.from(sender),
            fundAmount
        );

        address[] memory parentRoute = new address[](1);
        parentRoute[0] = route[0];
        SubnetID memory parentSubnetId = SubnetID(ROOTNET_CHAINID, parentRoute);

        require(fundMsg.from.subnetId.toHash() == parentSubnetId.toHash());
        require(fundMsg.from.rawAddress.extractEvmAddress() == sender);
        require(fundMsg.to.subnetId.toHash() == subnetId.toHash());
        require(fundMsg.to.rawAddress.extractEvmAddress() == sender);
        require(fundMsg.value == fundAmount);
        require(fundMsg.nonce == 0);
        require(fundMsg.kind == IpcMsgKind.Transfer);
    }

    function test_CreateFundMsg_Fails_SubnetNoParent(uint256 fundAmount, address sender) public {
        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, new address[](0));

        vm.expectRevert(NoParentForSubnet.selector);

        CrossMsgHelper.createFundMsg(subnetId, sender, FvmAddressHelper.from(sender), fundAmount);
    }

    function test_Execute_Works_SendValue() public {
        address sender = address(this);
        address recipient = address(100);

        crossMsg.to.rawAddress = FvmAddressHelper.from(recipient);
        crossMsg.kind = IpcMsgKind.Call;
        CallMsg memory message = crossMsg.getCallMsg();
        message.method = abi.encodePacked(METHOD_SEND);
        crossMsg.value = 1;
        crossMsg = crossMsg.setCallMsg(message);

        vm.deal(sender, 1 ether);

        (, bytes memory result) = crossMsg.execute(SupplySourceHelper.native());

        require(keccak256(result) == keccak256(EMPTY_BYTES));
        require(recipient.balance == 1);
        require(sender.balance == 1 ether - 1);
    }

    function test_Execute_Works_FunctionCallWithValue() public {
        address sender = address(this);
        address recipient = address(100);

        crossMsg.to.rawAddress = FvmAddressHelper.from(recipient);
        crossMsg.kind = IpcMsgKind.Call;
        CallMsg memory message = crossMsg.getCallMsg();
        message.method = abi.encodePacked(METHOD_SEND);
        crossMsg.value = 1;
        message.params = abi.encode(EMPTY_BYTES);
        crossMsg = crossMsg.setCallMsg(message);

        vm.deal(sender, 1 ether);
        vm.expectCall(recipient, crossMsg.value, new bytes(0), 1);

        (, bytes memory result) = crossMsg.execute(SupplySourceHelper.native());

        require(keccak256(result) == keccak256(EMPTY_BYTES));
    }

    function test_Execute_Works_FunctionCallWithoutValue() public {
        address sender = address(this);
        address recipient = address(100);

        crossMsg.kind = IpcMsgKind.Call;
        crossMsg.to.rawAddress = FvmAddressHelper.from(recipient);
        CallMsg memory message = crossMsg.getCallMsg();
        message.method = abi.encodePacked(METHOD_SEND);
        crossMsg.value = 0;
        message.params = abi.encode(EMPTY_BYTES);
        crossMsg = crossMsg.setCallMsg(message);

        vm.deal(sender, 1 ether);
        vm.expectCall(recipient, crossMsg.value, new bytes(0), 1);

        (, bytes memory result) = crossMsg.execute(SupplySourceHelper.native());

        require(keccak256(result) == keccak256(EMPTY_BYTES));
    }

    function test_Execute_Fails_InvalidMethod() public {
        SupplySource memory native = SupplySourceHelper.native();

        crossMsg.kind = IpcMsgKind.Call;
        crossMsg.to.rawAddress = FvmAddressHelper.from(address(this));
        CallMsg memory message = crossMsg.getCallMsg();
        message.method = bytes("1");
        crossMsg = crossMsg.setCallMsg(message);

        (bool success, ) = crossMsg.execute(native);
        require(!success);
    }

    function callback(bytes calldata params) public payable returns (bytes memory) {
        return params;
    }

    function callbackWrapped(IpcEnvelope memory w) public payable returns (bytes memory) {
        return abi.encode(w);
    }

    function test_IsSorted_Works_SingleMsg() public {
        addCrossMsg(0);

        require(CrossMsgHelper.isSorted(crossMsgs));
    }

    function test_IsSorted_Works_MultipleMsgsSorted() public {
        addCrossMsg(0);
        addCrossMsg(1);

        require(CrossMsgHelper.isSorted(crossMsgs));
    }

    function test_IsSorted_Works_MultipleMsgsNotSorted() public {
        addCrossMsg(0);
        addCrossMsg(2);
        addCrossMsg(1);

        require(CrossMsgHelper.isSorted(crossMsgs) == false);
    }

    function test_IsSorted_Works_MultipleMsgsZeroNonces() public {
        addCrossMsg(0);
        addCrossMsg(0);

        require(CrossMsgHelper.isSorted(crossMsgs) == false);
    }

    function test_applyType_TopDown() public pure {
        address[] memory from = new address[](1);
        from[0] = address(1);
        address[] memory to = new address[](4);
        to[0] = address(1);
        to[1] = address(2);
        to[2] = address(3);
        to[3] = address(4);

        IPCAddress memory ifrom = IPCAddress({
            subnetId: SubnetID({root: ROOTNET_CHAINID, route: from}),
            rawAddress: FvmAddressHelper.from(address(3))
        });
        IPCAddress memory ito = IPCAddress({
            subnetId: SubnetID({root: ROOTNET_CHAINID, route: to}),
            rawAddress: FvmAddressHelper.from(address(3))
        });

        IpcEnvelope memory storableMsg = createTransferMsg(ifrom, ito, 0);

        require(
            storableMsg.applyType(SubnetID({root: ROOTNET_CHAINID, route: from})) == IPCMsgType.TopDown,
            "Should be TopDown"
        );

        address[] memory current = new address[](2);
        current[0] = address(1);
        current[1] = address(2);
        SubnetID memory subnetId = SubnetID({root: ROOTNET_CHAINID, route: current});

        require(storableMsg.applyType(subnetId) == IPCMsgType.TopDown, "Should be TopDown");

        address[] memory current2 = new address[](3);
        current2[0] = address(1);
        current2[1] = address(2);
        current2[2] = address(3);

        require(
            storableMsg.applyType(SubnetID({root: ROOTNET_CHAINID, route: current2})) == IPCMsgType.TopDown,
            "Should be TopDown"
        );
    }

    function test_applyType_BottomUp() public pure {
        address[] memory from = new address[](2);
        from[0] = address(1);
        from[1] = address(2);
        address[] memory to = new address[](1);
        to[0] = address(1);

        IPCAddress memory ifrom = IPCAddress({
            subnetId: SubnetID({root: ROOTNET_CHAINID, route: from}),
            rawAddress: FvmAddressHelper.from(address(3))
        });
        IPCAddress memory ito = IPCAddress({
            subnetId: SubnetID({root: ROOTNET_CHAINID, route: to}),
            rawAddress: FvmAddressHelper.from(address(3))
        });

        IpcEnvelope memory storableMsg = createTransferMsg(ifrom, ito, 0);

        require(
            storableMsg.applyType(SubnetID({root: ROOTNET_CHAINID, route: from})) == IPCMsgType.BottomUp,
            "Should be BottomUp"
        );
        require(
            storableMsg.applyType(SubnetID({root: ROOTNET_CHAINID, route: to})) == IPCMsgType.BottomUp,
            "Should be BottomUp"
        );
    }

    function createDefaultTransferMsg(uint64 nonce) internal pure returns (IpcEnvelope memory) {
        IPCAddress memory addr = IPCAddress({
            subnetId: SubnetID(0, new address[](0)),
            rawAddress: FvmAddressHelper.from(address(0))
        });
        return createTransferMsg(addr, addr, nonce);
    }

    function createTransferMsg(
        IPCAddress memory from,
        IPCAddress memory to,
        uint64 nonce
    ) internal pure returns (IpcEnvelope memory) {
        return
            IpcEnvelope({kind: IpcMsgKind.Transfer, from: from, to: to, value: 0, message: EMPTY_BYTES, nonce: nonce});
    }

    function createCrossMsgs(uint256 length, uint64 nonce) internal pure returns (IpcEnvelope[] memory _crossMsgs) {
        _crossMsgs = new IpcEnvelope[](length);

        for (uint256 i = 0; i < length; i++) {
            _crossMsgs[i] = createDefaultTransferMsg(nonce);
        }
    }

    function addCrossMsg(uint64 nonce) internal {
        crossMsg.nonce = nonce;

        crossMsgs.push(crossMsg);
    }
}

library CallMsgTestHelper {
    error InvalidCrossMsgKind();

    // get underlying IpcMsg from crossMsg
    function getCallMsg(IpcEnvelope memory envelope) internal pure returns (CallMsg memory ret) {
        if (CrossMsgHelper.isEmpty(envelope)) {
            return ret;
        }
        if (envelope.kind == IpcMsgKind.Call) {
            CallMsg memory message = abi.decode(envelope.message, (CallMsg));
            return message;
        }

        // return empty IpcMsg otherwise
        return ret;
    }

    // set underlying IpcMsg from crossMsg.
    // This is a pure function, so the argument is not mutated
    function setCallMsg(
        IpcEnvelope memory envelope,
        CallMsg memory message
    ) internal pure returns (IpcEnvelope memory ret) {
        if (envelope.kind == IpcMsgKind.Call) {
            envelope.message = abi.encode(message);
            return envelope;
        }

        // Cannot set CallMsg for the wrong kind
        revert InvalidCrossMsgKind();
    }
}
