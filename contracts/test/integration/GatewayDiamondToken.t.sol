// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";

import "../../src/errors/IPCErrors.sol";
import {EMPTY_BYTES, METHOD_SEND, EMPTY_HASH} from "../../src/constants/Constants.sol";
import {IpcEnvelope, BottomUpMsgBatch, BottomUpCheckpoint} from "../../src/structs/CrossNet.sol";
import {FvmAddress} from "../../src/structs/FvmAddress.sol";
import {IPCAddress, SubnetID, Subnet, SupplySource, SupplyKind, Validator} from "../../src/structs/Subnet.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";
import {FvmAddressHelper} from "../../src/lib/FvmAddressHelper.sol";
import {CrossMsgHelper} from "../../src/lib/CrossMsgHelper.sol";
import {IIpcHandler} from "../../sdk/interfaces/IIpcHandler.sol";
import {SupplySourceHelper} from "../../src/lib/SupplySourceHelper.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {GatewayDiamond} from "../../src/GatewayDiamond.sol";
import {LibGateway} from "../../src/lib/LibGateway.sol";
import {MockIpcContract, TestUtils} from "../helpers/TestUtils.sol";
import {IntegrationTestBase} from "../IntegrationTestBase.sol";
import {SubnetActorDiamond} from "../../src/SubnetActorDiamond.sol";
import {GatewayGetterFacet} from "../../src/gateway/GatewayGetterFacet.sol";
import {GatewayMessengerFacet} from "../../src/gateway/GatewayMessengerFacet.sol";
import {GatewayManagerFacet} from "../../src/gateway/GatewayManagerFacet.sol";
import {SubnetActorManagerFacet} from "../../src/subnet/SubnetActorManagerFacet.sol";
import {SubnetActorGetterFacet} from "../../src/subnet/SubnetActorGetterFacet.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";

import {IERC20} from "openzeppelin-contracts/token/ERC20/IERC20.sol";
import {ERC20PresetFixedSupply} from "../helpers/ERC20PresetFixedSupply.sol";
import {IERC20Errors} from "openzeppelin-contracts/interfaces/draft-IERC6093.sol";

import {GatewayFacetsHelper} from "../helpers/GatewayFacetsHelper.sol";

contract GatewayDiamondTokenTest is Test, IntegrationTestBase {
    using SubnetIDHelper for SubnetID;
    using CrossMsgHelper for IpcEnvelope;
    using FvmAddressHelper for FvmAddress;
    using GatewayFacetsHelper for GatewayDiamond;

    IERC20 private token;

    function setUp() public override {
        super.setUp();

        token = new ERC20PresetFixedSupply("TestToken", "TEST", 1_000_000, address(this));
    }

    function test_fundWithToken_NativeSupply_Reverts() public {
        (address validatorAddress, bytes memory publicKey) = TestUtils.deriveValidatorAddress(100);
        join(validatorAddress, publicKey);

        address caller = vm.addr(1);
        vm.deal(caller, 100);

        (SubnetID memory subnetId, , , , ) = getSubnet(address(saDiamond));

        vm.prank(caller);
        vm.expectRevert(SupplySourceHelper.UnexpectedSupplySource.selector);
        gatewayDiamond.manager().fundWithToken(subnetId, FvmAddressHelper.from(caller), 100);
    }

    function test_fund_TokenSupply_Reverts() public {
        address caller = vm.addr(1);
        vm.deal(caller, 100);

        Subnet memory subnet = createTokenSubnet(address(token));

        vm.prank(caller);
        vm.expectRevert(SupplySourceHelper.UnexpectedSupplySource.selector);
        gatewayDiamond.manager().fund{value: 100}(subnet.id, FvmAddressHelper.from(caller));
    }

    function testFail_InexistentToken() public {
        // Reverts because the token doesn't exist at that address.
        address addr = vm.addr(999);
        createTokenSubnet(addr);
    }

    function test_fundWithToken_FailsInsufficientBalance() public {
        Subnet memory subnet = createTokenSubnet(address(token));

        // account has native balance but no tokens, reverts.
        address caller = vm.addr(1);
        vm.deal(caller, 100);
        vm.prank(caller);
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientAllowance.selector, address(gatewayDiamond), 0, 1)
        );
        gatewayDiamond.manager().fundWithToken(subnet.id, FvmAddressHelper.from(caller), 1);
    }

    function test_fundWithToken() public {
        Subnet memory subnet = createTokenSubnet(address(token));

        address caller = vm.addr(1);
        vm.deal(caller, 100);
        token.transfer(caller, 100);
        vm.startPrank(caller);

        // Caller approves the gateway to spend funds on their behalf.
        token.approve(address(gatewayDiamond), 10);

        // Funding succeeds and the right event is emitted.
        IpcEnvelope memory expected = CrossMsgHelper.createFundMsg(
            subnet.id,
            caller,
            FvmAddressHelper.from(caller),
            10
        );
        vm.expectEmit(true, true, true, true, address(gatewayDiamond));
        emit LibGateway.NewTopDownMessage(address(saDiamond), expected);
        gatewayDiamond.manager().fundWithToken(subnet.id, FvmAddressHelper.from(caller), 10);

        // Assert post-conditions.
        (, Subnet memory subnetAfter) = gatewayDiamond.getter().getSubnet(subnet.id);
        assertEq(subnetAfter.circSupply, 10);
        assertEq(subnetAfter.topDownNonce, 1);

        // A new funding attempt with exhausted token balance should fail.
        vm.expectRevert();
        gatewayDiamond.manager().fundWithToken(subnet.id, FvmAddressHelper.from(caller), 10);

        // And the subnet's state should not have been updated.
        (, subnetAfter) = gatewayDiamond.getter().getSubnet(subnet.id);
        assertEq(subnetAfter.circSupply, 10);
        assertEq(subnetAfter.topDownNonce, 1);

        // After topping up it succeeds again.
        token.approve(address(gatewayDiamond), 5);
        gatewayDiamond.manager().fundWithToken(subnet.id, FvmAddressHelper.from(caller), 5);

        // And the subnet's bookkeeping is correct.
        (, subnetAfter) = gatewayDiamond.getter().getSubnet(subnet.id);
        assertEq(subnetAfter.circSupply, 15);
        assertEq(subnetAfter.topDownNonce, 2);
    }

    event Transfer(address indexed from, address indexed to, uint256 value);

    function test_withdrawToken_Parent() public {
        Subnet memory subnet = createTokenSubnet(address(token));

        // Fund an account in the subnet.
        address caller = vm.addr(1);
        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(address(gatewayDiamond), 15);
        vm.prank(caller);
        gatewayDiamond.manager().fundWithToken(subnet.id, FvmAddressHelper.from(caller), 15);

        // Now create a new recipient on the parent.
        address recipient = vm.addr(42);

        // Commit the withdrawal message on the parent.
        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        uint256 value = 8;
        msgs[0] = CrossMsgHelper.createReleaseMsg(subnet.id, caller, FvmAddressHelper.from(recipient), value);

        BottomUpCheckpoint memory batch = BottomUpCheckpoint({
            subnetID: subnet.id,
            blockHash: blockhash(block.number),
            blockHeight: gatewayDiamond.getter().bottomUpCheckPeriod(),
            nextConfigurationNumber: 0,
            msgs: msgs
        });

        vm.prank(address(saDiamond));
        vm.expectEmit(true, true, true, true, address(token));
        emit Transfer(address(gatewayDiamond), recipient, value);
        gatewayDiamond.checkpointer().commitCheckpoint(batch);

        // Assert post-conditions.
        (, Subnet memory subnetAfter) = gatewayDiamond.getter().getSubnet(subnet.id);
        assertEq(subnetAfter.circSupply, 7);
        assertEq(subnetAfter.topDownNonce, 2); // 2 because the result msg is also another td message
        assertEq(subnetAfter.appliedBottomUpNonce, 1);

        // Now attempt to withdraw beyond the circulating supply.
        // This would be a malicious message.
        batch.msgs[0] = CrossMsgHelper.createReleaseMsg(subnet.id, caller, FvmAddressHelper.from(recipient), 10);

        // This reverts.
        vm.prank(address(saDiamond));
        vm.expectRevert();
        gatewayDiamond.checkpointer().commitCheckpoint(batch);
    }

    // Call a smart contract in the parent through a smart contract and with
    // an ERC20 token supply.
    function test_childToParentCall() public {
        Subnet memory subnet = createTokenSubnet(address(token));

        // Fund an account in the subnet.
        address caller = vm.addr(1);
        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(address(gatewayDiamond), 15);
        vm.prank(caller);
        gatewayDiamond.manager().fundWithToken(subnet.id, FvmAddressHelper.from(caller), 15);

        // Now create a new recipient on the parent.
        address recipient = address(new MockIpcContract());

        // Commit a xnet message that isn't a simple bare transfer.
        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        uint256 value = 8;

        IPCAddress memory from = IPCAddress({subnetId: subnet.id, rawAddress: FvmAddressHelper.from(caller)});
        IPCAddress memory to = IPCAddress({
            subnetId: subnet.id.getParentSubnet(),
            rawAddress: FvmAddressHelper.from(recipient)
        });
        bytes4 method = bytes4(0x11223344);
        bytes memory params = bytes("hello");
        msgs[0] = CrossMsgHelper.createCallMsg(from, to, value, method, params);

        BottomUpCheckpoint memory batch = BottomUpCheckpoint({
            subnetID: subnet.id,
            blockHash: blockhash(block.number),
            blockHeight: gatewayDiamond.getter().bottomUpCheckPeriod(),
            nextConfigurationNumber: 0,
            msgs: msgs
        });

        // Verify that we received the call and that the recipient has the tokens.
        vm.prank(address(saDiamond));
        vm.expectCall(recipient, abi.encodeCall(IIpcHandler.handleIpcMessage, (msgs[0])), 1);
        gatewayDiamond.checkpointer().commitCheckpoint(batch);
        assertEq(token.balanceOf(recipient), 8);
    }

    function test_propagation() public {
        // TODO:
        // 1. Test that propagation is rejected when sender is ERC20.
        // 2. Test that propagation is rejected when receiver is ERC20.
        // 3. Test that propagation is rejected when an intermediary subnet is ERC20.
    }

    function createTokenSubnet(address tokenAddress) internal returns (Subnet memory) {
        // Create a subnet actor in the root network, with an ERC20 supply source with the specified token address.
        SubnetActorDiamond.ConstructorParams memory saConstructorParams = defaultSubnetActorParamsWith(
            address(gatewayDiamond)
        );
        saConstructorParams.supplySource = SupplySource({kind: SupplyKind.ERC20, tokenAddress: tokenAddress});

        // Override the state variables with the new subnet.
        saDiamond = createSubnetActor(saConstructorParams);

        // increment the block number by 5 (could be other number as well) so that commit
        // parent finality called down stream will work we need this because in setUp,
        // parent finality is committed at the block height, without
        // incrementing the block number, test won't pass
        vm.roll(5);

        addValidator(TOPDOWN_VALIDATOR_1, 100);

        (address validatorAddress, bytes memory publicKey) = TestUtils.deriveValidatorAddress(100);
        join(validatorAddress, publicKey);

        SubnetID memory subnetId = gatewayDiamond.getter().getNetworkName().createSubnetId(address(saDiamond));

        (bool exists, Subnet memory subnet) = gatewayDiamond.getter().getSubnet(subnetId);
        assert(exists);
        return subnet;
    }
}
