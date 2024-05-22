// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "../../src/errors/IPCErrors.sol";
import {EMPTY_BYTES, METHOD_SEND} from "../../src/constants/Constants.sol";
import {IpcEnvelope} from "../../src/structs/CrossNet.sol";
import {FvmAddress} from "../../src/structs/FvmAddress.sol";
import {SubnetID, Subnet, IPCAddress, Validator} from "../../src/structs/Subnet.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";
import {FvmAddressHelper} from "../../src/lib/FvmAddressHelper.sol";
import {CrossMsgHelper} from "../../src/lib/CrossMsgHelper.sol";
import {GatewayDiamond, FEATURE_MULTILEVEL_CROSSMSG} from "../../src/GatewayDiamond.sol";
import {GatewayGetterFacet} from "../../src/gateway/GatewayGetterFacet.sol";
import {GatewayManagerFacet} from "../../src/gateway/GatewayManagerFacet.sol";
import {XnetMessagingFacet} from "../../src/gateway/router/XnetMessagingFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {GatewayMessengerFacet} from "../../src/gateway/GatewayMessengerFacet.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {IntegrationTestBase} from "../IntegrationTestBase.sol";
import {L2GatewayActorDiamond} from "../IntegrationTestPresets.sol";
import {TestUtils} from "../helpers/TestUtils.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";

import {GatewayFacetsHelper} from "../helpers/GatewayFacetsHelper.sol";

contract L2GatewayActorDiamondTest is Test, L2GatewayActorDiamond {
    using SubnetIDHelper for SubnetID;
    using CrossMsgHelper for IpcEnvelope;
    using GatewayFacetsHelper for GatewayDiamond;

    function testGatewayDiamond_CommitParentFinality_BigNumberOfMessages() public {
        uint256 n = 2000;
        FvmAddress[] memory validators = new FvmAddress[](1);
        validators[0] = FvmAddressHelper.from(vm.addr(100));
        address receipient = vm.addr(102);
        vm.deal(vm.addr(100), 1);

        uint256[] memory weights = new uint[](1);
        weights[0] = 100;

        SubnetID memory id = gatewayDiamond.getter().getNetworkName();

        IpcEnvelope[] memory topDownMsgs = new IpcEnvelope[](n);
        for (uint64 i = 0; i < n; i++) {
            topDownMsgs[i] = TestUtils.newXnetCallMsg(
                IPCAddress({subnetId: id, rawAddress: FvmAddressHelper.from(address(this))}),
                IPCAddress({
                    subnetId: gatewayDiamond.getter().getNetworkName().getParentSubnet(),
                    rawAddress: FvmAddressHelper.from(receipient)
                }),
                0,
                i
            );
        }

        vm.startPrank(FilAddress.SYSTEM_ACTOR);

        gatewayDiamond.xnetMessenger().applyCrossMessages(topDownMsgs);
        require(gatewayDiamond.getter().getSubnetTopDownMsgsLength(id) == 0, "unexpected top-down message");
        (bool ok, uint64 tdn) = gatewayDiamond.getter().getTopDownNonce(id);
        require(!ok && tdn == 0, "unexpected nonce");

        vm.stopPrank();
    }

    function testGatewayDiamond_Propagate_Works_WithFeeRemainderNew() external {
        if (!FEATURE_MULTILEVEL_CROSSMSG) {
            // skip
            return;
        }
        (, address[] memory validators) = setupValidators();
        address caller = validators[0];

        bytes32 postboxId = setupWhiteListMethod(caller);

        vm.deal(caller, 1 ether);

        vm.expectCall(caller, 1 ether, new bytes(0), 1);
        vm.prank(caller);
        gatewayDiamond.messenger().propagate{value: 1 ether}(postboxId);

        require(caller.balance == 1 ether, "unexpected balance");
    }

    function testGatewayDiamond_Propagate_Works_NoFeeReminder() external {
        if (!FEATURE_MULTILEVEL_CROSSMSG) {
            // skip
            return;
        }
        (, address[] memory validators) = setupValidators();
        address caller = validators[0];

        bytes32 postboxId = setupWhiteListMethod(caller);

        vm.prank(caller);
        vm.expectCall(caller, 0, EMPTY_BYTES, 0);
        gatewayDiamond.messenger().propagate{value: 0}(postboxId);
        require(caller.balance == 0, "unexpected balance");
    }

    function setupWhiteListMethod(address caller) internal returns (bytes32) {
        registerSubnet(DEFAULT_COLLATERAL_AMOUNT, address(this));

        IpcEnvelope memory crossMsg = TestUtils.newXnetCallMsg(
            IPCAddress({
                subnetId: gatewayDiamond.getter().getNetworkName().createSubnetId(caller),
                rawAddress: FvmAddressHelper.from(caller)
            }),
            IPCAddress({
                subnetId: gatewayDiamond.getter().getNetworkName().createSubnetId(address(this)),
                rawAddress: FvmAddressHelper.from(address(this))
            }),
            DEFAULT_CROSS_MSG_FEE + 1,
            0
        );
        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = crossMsg;

        vm.prank(FilAddress.SYSTEM_ACTOR);
        gatewayDiamond.xnetMessenger().applyCrossMessages(msgs);

        return crossMsg.toHash();
    }

    function callback() public view {}
}
