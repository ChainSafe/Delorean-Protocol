// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "../../src/errors/IPCErrors.sol";
import {EMPTY_BYTES, METHOD_SEND} from "../../src/constants/Constants.sol";
import {IpcEnvelope, BottomUpMsgBatch, BottomUpCheckpoint, ParentFinality, IpcMsgKind, OutcomeType} from "../../src/structs/CrossNet.sol";
import {FvmAddress} from "../../src/structs/FvmAddress.sol";
import {SubnetID, Subnet, IPCAddress, Validator} from "../../src/structs/Subnet.sol";
import {SubnetIDHelper} from "../../src/lib/SubnetIDHelper.sol";
import {SupplySourceHelper} from "../../src/lib/SupplySourceHelper.sol";
import {FvmAddressHelper} from "../../src/lib/FvmAddressHelper.sol";
import {CrossMsgHelper} from "../../src/lib/CrossMsgHelper.sol";
import {GatewayDiamond, FEATURE_MULTILEVEL_CROSSMSG} from "../../src/GatewayDiamond.sol";
import {SubnetActorDiamond} from "../../src/SubnetActorDiamond.sol";
import {SubnetActorGetterFacet} from "../../src/subnet/SubnetActorGetterFacet.sol";
import {SubnetActorManagerFacet} from "../../src/subnet/SubnetActorManagerFacet.sol";
import {SubnetActorCheckpointingFacet} from "../../src/subnet/SubnetActorCheckpointingFacet.sol";
import {GatewayGetterFacet} from "../../src/gateway/GatewayGetterFacet.sol";
import {GatewayManagerFacet} from "../../src/gateway/GatewayManagerFacet.sol";
import {LibGateway} from "../../src/lib/LibGateway.sol";
import {TopDownFinalityFacet} from "../../src/gateway/router/TopDownFinalityFacet.sol";
import {CheckpointingFacet} from "../../src/gateway/router/CheckpointingFacet.sol";
import {XnetMessagingFacet} from "../../src/gateway/router/XnetMessagingFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {GatewayMessengerFacet} from "../../src/gateway/GatewayMessengerFacet.sol";
import {DiamondLoupeFacet} from "../../src/diamond/DiamondLoupeFacet.sol";
import {DiamondCutFacet} from "../../src/diamond/DiamondCutFacet.sol";
import {IntegrationTestBase, RootSubnetDefinition, TestSubnetDefinition} from "../IntegrationTestBase.sol";
import {L2GatewayActorDiamond, L1GatewayActorDiamond} from "../IntegrationTestPresets.sol";
import {TestUtils, MockIpcContract, MockIpcContractPayable, MockIpcContractRevert, MockIpcContractFallback} from "../helpers/TestUtils.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {MerkleTreeHelper} from "../helpers/MerkleTreeHelper.sol";

import {IERC20} from "openzeppelin-contracts/token/ERC20/IERC20.sol";
import {ERC20PresetFixedSupply} from "../helpers/ERC20PresetFixedSupply.sol";
import {ERC20Deflationary} from "../helpers/ERC20Deflationary.sol";
import {ERC20Inflationary} from "../helpers/ERC20Inflationary.sol";
import {ERC20Nil} from "../helpers/ERC20Nil.sol";

import {IERC20Errors} from "openzeppelin-contracts/interfaces/draft-IERC6093.sol";

import {GatewayFacetsHelper} from "../helpers/GatewayFacetsHelper.sol";
import {SubnetActorFacetsHelper} from "../helpers/SubnetActorFacetsHelper.sol";

import "forge-std/console.sol";

contract MultiSubnetTest is Test, IntegrationTestBase {
    using SubnetIDHelper for SubnetID;
    using CrossMsgHelper for IpcEnvelope;
    using GatewayFacetsHelper for GatewayDiamond;
    using SubnetActorFacetsHelper for SubnetActorDiamond;

    RootSubnetDefinition public rootSubnet;
    TestSubnetDefinition public nativeSubnet;
    TestSubnetDefinition public tokenSubnet;
    TestSubnetDefinition public deflationaryTokenSubnet;
    TestSubnetDefinition public inflationaryTokenSubnet;
    TestSubnetDefinition public nilTokenSubnet;

    IERC20 public token;
    IERC20 public deflationaryToken;
    IERC20 public inflationaryToken;
    IERC20 public nilToken;

    function setUp() public override {
        SubnetID memory rootSubnetName = SubnetID({root: ROOTNET_CHAINID, route: new address[](0)});
        require(rootSubnetName.isRoot(), "not root");

        GatewayDiamond rootGateway = createGatewayDiamond(gatewayParams(rootSubnetName));

        SubnetActorDiamond rootNativeSubnetActor = createSubnetActor(
            defaultSubnetActorParamsWith(address(rootGateway), rootSubnetName)
        );

        address[] memory nativeSubnetPath = new address[](1);
        nativeSubnetPath[0] = address(rootNativeSubnetActor);
        SubnetID memory nativeSubnetName = SubnetID({root: ROOTNET_CHAINID, route: nativeSubnetPath});
        GatewayDiamond nativeSubnetGateway = createGatewayDiamond(gatewayParams(nativeSubnetName));

        rootSubnet = RootSubnetDefinition({
            gateway: rootGateway,
            gatewayAddr: address(rootGateway),
            id: rootSubnetName
        });

        nativeSubnet = TestSubnetDefinition({
            gateway: nativeSubnetGateway,
            gatewayAddr: address(nativeSubnetGateway),
            id: nativeSubnetName,
            subnetActor: rootNativeSubnetActor,
            subnetActorAddr: address(rootNativeSubnetActor),
            path: nativeSubnetPath
        });

        token = new ERC20PresetFixedSupply("TestToken", "TEST", 1_000_000, address(this));
        tokenSubnet = createTokenSubnet(address(token), address(rootGateway), rootSubnetName);

        deflationaryToken = new ERC20Deflationary("DeflationaryToken", "DFT", 1_000_000, address(this), 50);
        deflationaryTokenSubnet = createTokenSubnet(address(deflationaryToken), address(rootGateway), rootSubnetName);

        inflationaryToken = new ERC20Inflationary("InflationaryToken", "IFT", 1_000_000, address(this), 100);
        inflationaryTokenSubnet = createTokenSubnet(address(inflationaryToken), address(rootGateway), rootSubnetName);

        nilToken = new ERC20Nil("NilToken", "NFT", 1_000_000, address(this));
        nilTokenSubnet = createTokenSubnet(address(nilToken), address(rootGateway), rootSubnetName);

        printActors();
    }

    function createTokenSubnet(
        address tokenAddress,
        address rootGatewayAddress,
        SubnetID memory rootSubnetName
    ) internal returns (TestSubnetDefinition memory tokenSubnet) {
        SubnetActorDiamond rootTokenSubnetActor = createSubnetActor(
            defaultSubnetActorParamsWith(rootGatewayAddress, rootSubnetName, tokenAddress)
        );
        address[] memory tokenSubnetPath = new address[](1);
        tokenSubnetPath[0] = address(rootTokenSubnetActor);
        SubnetID memory tokenSubnetName = SubnetID({root: ROOTNET_CHAINID, route: tokenSubnetPath});
        GatewayDiamond tokenSubnetGateway = createGatewayDiamond(gatewayParams(tokenSubnetName));

        tokenSubnet = TestSubnetDefinition({
            gateway: tokenSubnetGateway,
            gatewayAddr: address(tokenSubnetGateway),
            id: tokenSubnetName,
            subnetActor: rootTokenSubnetActor,
            subnetActorAddr: address(rootTokenSubnetActor),
            path: tokenSubnetPath
        });
    }

    //--------------------
    // Fund flow tests.
    //---------------------

    function testMultiSubnet_Native_FundingFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, amount);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory expected = CrossMsgHelper.createFundMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );

        vm.prank(caller);
        vm.expectEmit(true, true, true, true, rootSubnet.gatewayAddr);
        emit LibGateway.NewTopDownMessage(nativeSubnet.subnetActorAddr, expected);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(recipient)));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = expected;

        // TODO: commitParentFinality doesn't not affect anything in this test.
        commitParentFinality(nativeSubnet.gatewayAddr);

        executeTopDownMsgs(msgs, nativeSubnet.id, nativeSubnet.gateway);

        assertEq(recipient.balance, amount);
    }

    // A bottom up receipt sending from parent to child. The original message is a
    // bottom up release message, but the execution worked in the parent, creating
    // a topdown result message from parent to child
    function testMultiSubnet_Native_OkResultFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 0);
        vm.deal(recipient, 0);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg(
            nativeSubnet.id,
            recipient,
            FvmAddressHelper.from(caller),
            amount
        );

        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.Ok, new bytes(0));
        require(resultMsg.value == 0, "ok receipt should have 0 value");

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = resultMsg;

        executeTopDownMsgs(msgs, nativeSubnet.id, nativeSubnet.gateway);

        // works with no state changes
        assertEq(recipient.balance, 0);
        assertEq(caller.balance, 0);
    }

    // A bottom up receipt sending from parent to child. The original message is a
    // bottom up release message, but the execution encounters system error in the parent,
    // creating a topdown result message from parent to child
    function testMultiSubnet_Native_SystemErrResultFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );

        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.SystemErr, new bytes(0));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = resultMsg;

        executeTopDownMsgs(msgs, nativeSubnet.id, nativeSubnet.gateway);

        require(caller.balance == amount, "refund not happening");
    }

    // A bottom up receipt sending from parent to child. The original message is a
    // bottom up release message, but the execution encounters system error in the parent,
    // creating a topdown result message from parent to child
    function testMultiSubnet_Native_ActorErrResultFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );

        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.ActorErr, new bytes(0));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = resultMsg;

        executeTopDownMsgs(msgs, nativeSubnet.id, nativeSubnet.gateway);

        require(caller.balance == amount, "refund not happening");
    }

    function testMultiSubnet_Native_NonPayable_FundingFromParentToChildFails() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractFallback());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, amount);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory expected = CrossMsgHelper.createFundMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );

        vm.prank(caller);
        vm.expectEmit(true, true, true, true, rootSubnet.gatewayAddr);
        emit LibGateway.NewTopDownMessage(nativeSubnet.subnetActorAddr, expected);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(recipient)));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = expected;

        // TODO: commitParentFinality doesn't not affect anything in this test.
        commitParentFinality(nativeSubnet.gatewayAddr);

        vm.expectRevert();
        executeTopDownMsgsRevert(msgs, nativeSubnet.id, nativeSubnet.gateway);
    }

    function testMultiSubnet_Erc20_FundingFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, amount);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory expected = CrossMsgHelper.createFundMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );

        vm.prank(caller);
        vm.expectEmit(true, true, true, true, rootSubnet.gatewayAddr);
        emit LibGateway.NewTopDownMessage(tokenSubnet.subnetActorAddr, expected);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(recipient)), amount);

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = expected;

        commitParentFinality(tokenSubnet.gatewayAddr);

        executeTopDownMsgs(msgs, tokenSubnet.id, tokenSubnet.gateway);

        assertEq(recipient.balance, amount);
    }

    function testMultiSubnet_DeflationaryErc20_ReleaseFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 4096;

        deflationaryToken.transfer(caller, amount);
        assertEq(deflationaryToken.balanceOf(caller), amount / 2);

        vm.prank(caller);
        deflationaryToken.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(deflationaryTokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(deflationaryTokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, deflationaryTokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        vm.expectRevert(); // half of tokens burned on transfer
        rootSubnet.gateway.manager().fundWithToken(
            deflationaryTokenSubnet.id,
            FvmAddressHelper.from(address(caller)),
            amount
        );

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(
            deflationaryTokenSubnet.id,
            FvmAddressHelper.from(address(caller)),
            amount / 2
        );

        assertEq(deflationaryToken.balanceOf(caller), 0);
        assertEq(deflationaryToken.balanceOf(rootSubnet.gatewayAddr), amount / 4);
        assertEq(getSubnetCircSupplyGW(deflationaryTokenSubnet.id, rootSubnet.gateway), amount / 4);

        GatewayManagerFacet manager = deflationaryTokenSubnet.gateway.manager();
        uint256 releaseAmount = amount / 2 / 2;

        vm.prank(caller);
        manager.release{value: releaseAmount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            deflationaryTokenSubnet.id,
            deflationaryTokenSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, deflationaryTokenSubnet.subnetActor);

        assertEq(deflationaryToken.balanceOf(recipient), releaseAmount / 2);
        assertEq(deflationaryToken.balanceOf(rootSubnet.gatewayAddr), 0);
        assertEq(getSubnetCircSupplyGW(deflationaryTokenSubnet.id, rootSubnet.gateway), 0);
    }

    function testMultiSubnet_InflationaryErc20_ReleaseFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 4096;

        inflationaryToken.transfer(caller, amount);
        vm.prank(caller);
        inflationaryToken.approve(rootSubnet.gatewayAddr, amount * 2);

        vm.deal(inflationaryTokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(inflationaryTokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, inflationaryTokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(
            inflationaryTokenSubnet.id,
            FvmAddressHelper.from(address(caller)),
            amount * 2
        );

        assertEq(inflationaryToken.balanceOf(caller), 0);
        assertEq(inflationaryToken.balanceOf(rootSubnet.gatewayAddr), amount * 4);
        assertEq(getSubnetCircSupplyGW(inflationaryTokenSubnet.id, rootSubnet.gateway), amount * 4);

        GatewayManagerFacet manager = inflationaryTokenSubnet.gateway.manager();
        uint256 releaseAmount = amount * 2 * 2;

        vm.prank(caller);
        manager.release{value: releaseAmount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            inflationaryTokenSubnet.id,
            inflationaryTokenSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, inflationaryTokenSubnet.subnetActor);

        assertEq(inflationaryToken.balanceOf(recipient), releaseAmount * 2);
        assertEq(inflationaryToken.balanceOf(rootSubnet.gatewayAddr), 0);
        assertEq(getSubnetCircSupplyGW(inflationaryTokenSubnet.id, rootSubnet.gateway), 0);
    }

    function testMultiSubnet_NilErc20_ReleaseFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 4096;

        nilToken.transfer(caller, amount);
        vm.prank(caller);
        nilToken.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(nilTokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nilTokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nilTokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        vm.expectRevert(SupplySourceHelper.NoBalanceIncrease.selector);
        rootSubnet.gateway.manager().fundWithToken(nilTokenSubnet.id, FvmAddressHelper.from(address(caller)), amount);
        assertEq(getSubnetCircSupplyGW(nilTokenSubnet.id, rootSubnet.gateway), 0);
    }

    function testMultiSubnet_Erc20_ReleaseResultOkFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, amount);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.Ok, new bytes(0));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = resultMsg;

        executeTopDownMsgs(msgs, tokenSubnet.id, tokenSubnet.gateway);

        assertEq(caller.balance, amount);
    }

    function testMultiSubnet_Erc20_ReleaseActorErrFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 0);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.ActorErr, new bytes(0));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = resultMsg;

        executeTopDownMsgs(msgs, tokenSubnet.id, tokenSubnet.gateway);
        require(caller.balance == amount, "refund should have happened");
    }

    function testMultiSubnet_Erc20_ReleaseSystemErrFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 0);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.SystemErr, new bytes(0));

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = resultMsg;

        executeTopDownMsgs(msgs, tokenSubnet.id, tokenSubnet.gateway);
        require(caller.balance == amount, "refund should have happened");
    }

    function testMultiSubnet_Erc20_NonPayable_FundingFromParentToChildFails() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractFallback());
        uint256 amount = 3;

        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, amount);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory expected = CrossMsgHelper.createFundMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );

        vm.prank(caller);
        vm.expectEmit(true, true, true, true, rootSubnet.gatewayAddr);
        emit LibGateway.NewTopDownMessage(tokenSubnet.subnetActorAddr, expected);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(recipient)), amount);

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = expected;

        commitParentFinality(tokenSubnet.gatewayAddr);

        vm.expectRevert();
        executeTopDownMsgsRevert(msgs, tokenSubnet.id, tokenSubnet.gateway);
    }

    //--------------------
    // Release flow tests.
    //---------------------

    function testMultiSubnet_Native_ReleaseFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));

        GatewayManagerFacet manager = GatewayManagerFacet(nativeSubnet.gatewayAddr);

        vm.prank(caller);
        manager.release{value: amount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);
        assertEq(recipient.balance, amount);
    }

    // The result message from the parent sending a fund message to the child.
    // The result message would be a bottom up cross message from the child to
    // the parent.
    function testMultiSubnet_Native_FundOkResultFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.Ok, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        GatewayManagerFacet manager = GatewayManagerFacet(nativeSubnet.gatewayAddr);

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway,
            crossMsgs
        );

        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);

        // no change to caller's balance
        assertEq(caller.balance, 1 ether);
    }

    // The result message from the parent sending a fund message to the child.
    // The result message would be a bottom up cross message from the child to
    // the parent.
    function testMultiSubnet_Native_FundActorErrResultFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        // fund first to provide circulation
        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));
        assertEq(caller.balance, 1 ether - amount);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.ActorErr, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        GatewayManagerFacet manager = GatewayManagerFacet(nativeSubnet.gatewayAddr);

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway,
            crossMsgs
        );

        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);

        // caller's balance should receive the refund
        assertEq(caller.balance, 1 ether);
    }

    // The result message from the parent sending a fund message to the child.
    // The result message would be a bottom up cross message from the child to
    // the parent.
    function testMultiSubnet_Native_FundSystemErrResultFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        // fund first to provide circulation
        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));
        assertEq(caller.balance, 1 ether - amount);

        // now the fund propagated to the child and execution is Ok
        // assuming the Result message is created in the child message batch
        // and relayer pushed to the parent

        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg(
            nativeSubnet.id,
            caller,
            FvmAddressHelper.from(recipient),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.SystemErr, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        GatewayManagerFacet manager = GatewayManagerFacet(nativeSubnet.gatewayAddr);

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway,
            crossMsgs
        );

        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);

        // caller's balance should receive the refund
        assertEq(caller.balance, 1 ether);
    }

    function testMultiSubnet_Native_NonPayable_ReleaseFromChildToParentFails() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractFallback());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 6);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));

        GatewayManagerFacet manager = GatewayManagerFacet(nativeSubnet.gatewayAddr);
        vm.prank(caller);
        manager.release{value: amount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway
        );

        vm.expectRevert();
        submitBottomUpCheckpointRevert(checkpoint, nativeSubnet.subnetActor);
    }

    function testMultiSubnet_Native_ReleaseFromChildToParent_DifferentFunderAndSenderInParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 6);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: amount}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));

        GatewayManagerFacet manager = GatewayManagerFacet(nativeSubnet.gatewayAddr);
        vm.prank(caller);
        manager.release{value: amount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);

        assertEq(recipient.balance, amount);
    }

    function testMultiSubnet_Erc20_FundResultOkFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, amount);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), amount);
        assertEq(token.balanceOf(caller), 0);

        // now the fund propagated to the child and execution is Ok
        // assuming the Result message is created in the child message batch
        // and relayer pushed to the parent

        // simulating the checkpoint pushed from relayer
        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(caller),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.Ok, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway,
            crossMsgs
        );

        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);

        // fund works, so fund is still locked, balance is 0
        assertEq(token.balanceOf(caller), 0);
    }

    function testMultiSubnet_Erc20_FundResultSystemErrFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, amount);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), amount);
        assertEq(token.balanceOf(caller), 0);

        // now the fund propagated to the child and execution is Ok
        // assuming the Result message is created in the child message batch
        // and relayer pushed to the parent

        // simulating the checkpoint pushed from relayer
        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(caller),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.SystemErr, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway,
            crossMsgs
        );

        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);

        // fund rejected, so fund should be unlocked
        assertEq(token.balanceOf(caller), amount);
    }

    function testMultiSubnet_Erc20_FundResultActorErrFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, amount);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), amount);
        assertEq(token.balanceOf(caller), 0);

        // now the fund propagated to the child and execution is Ok
        // assuming the Result message is created in the child message batch
        // and relayer pushed to the parent

        // simulating the checkpoint pushed from relayer
        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg(
            tokenSubnet.id,
            caller,
            FvmAddressHelper.from(caller),
            amount
        );
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.ActorErr, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway,
            crossMsgs
        );

        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);

        // fund rejected, so fund should be unlocked
        assertEq(token.balanceOf(caller), amount);
    }

    function testMultiSubnet_Erc20_ReleaseFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractPayable());
        uint256 amount = 3;

        token.transfer(caller, amount);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), amount);

        GatewayManagerFacet manager = tokenSubnet.gateway.manager();
        vm.prank(caller);
        manager.release{value: amount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);

        assertEq(token.balanceOf(recipient), amount);
    }

    function testMultiSubnet_Erc20_Transfer_NonPayable_ReleaseFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContractFallback());
        uint256 amount = 3;

        token.transfer(caller, amount);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, amount);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), amount);

        GatewayManagerFacet manager = tokenSubnet.gateway.manager();
        vm.prank(caller);
        manager.release{value: amount}(FvmAddressHelper.from(address(recipient)));

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);
        assertEq(token.balanceOf(recipient), amount);
        assertEq(recipient.balance, 0);
    }

    //--------------------
    // Call flow tests.
    //---------------------

    function testMultiSubnet_Native_CallResultRevertsFromChildToParent() public {
        address caller = address(new MockIpcContractRevert());
        address recipient = address(new MockIpcContractRevert());
        uint256 amount = 3;

        uint256 initialBalance = 1 ether;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, initialBalance);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        require(rootSubnet.gatewayAddr.balance == initialBalance, "initial balance not correct");

        uint256 fundToSend = 100000;

        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: fundToSend}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));

        require(
            address(rootSubnet.gateway.manager()).balance == initialBalance + fundToSend,
            "fund not locked in gateway"
        );

        // a cross network message from parent caller to child recipient
        IpcEnvelope memory crossMsg = TestUtils.newXnetCallMsg(
            IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
            IPCAddress({subnetId: nativeSubnet.id, rawAddress: FvmAddressHelper.from(recipient)}),
            amount,
            0
        );
        // result in child is ActorErr, resultMsg should be a bottom up message
        // from child recipient to the parent caller
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.ActorErr, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway,
            crossMsgs
        );

        // execution should be in the root native subnet, in the parent
        // note that the result msg is sent to the caller in the parent,
        // but the caller is MockIpcContractRevert, which reverts whatever
        // call made to `handleIpcMessage`, we should make sure:
        // 1. The submission of checkpoint is never blocked
        // 2. The fund is locked in the gateway as execution is rejected
        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);

        require(
            address(rootSubnet.gateway.manager()).balance == initialBalance + fundToSend,
            "fund should still be locked in gateway"
        );
        require(caller.balance == initialBalance - fundToSend, "fund should still be locked in gateway");
    }

    function testMultiSubnet_Native_SendCrossMessageFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContract());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: 100000}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));

        GatewayMessengerFacet messenger = nativeSubnet.gateway.messenger();
        vm.prank(address(caller));
        messenger.sendContractXnetMessage{value: amount}(
            TestUtils.newXnetCallMsg(
                IPCAddress({subnetId: nativeSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
                IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(recipient)}),
                amount,
                0
            )
        );

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnet.id,
            nativeSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, nativeSubnet.subnetActor);

        assertEq(recipient.balance, amount);
    }

    function testMultiSubnet_Native_SendCrossMessageFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContract());
        uint256 amount = 3;

        vm.deal(nativeSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        vm.prank(nativeSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, nativeSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fund{value: 100000}(nativeSubnet.id, FvmAddressHelper.from(address(caller)));

        IpcEnvelope memory xnetCallMsg = TestUtils.newXnetCallMsg(
            IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
            IPCAddress({subnetId: nativeSubnet.id, rawAddress: FvmAddressHelper.from(recipient)}),
            amount,
            0
        );

        IpcEnvelope memory committedEvent = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
            to: xnetCallMsg.to,
            value: xnetCallMsg.value,
            message: xnetCallMsg.message,
            nonce: 1
        });

        vm.prank(address(caller));
        vm.expectEmit(true, true, true, true, rootSubnet.gatewayAddr);
        emit LibGateway.NewTopDownMessage({subnet: nativeSubnet.subnetActorAddr, message: committedEvent});
        rootSubnet.gateway.messenger().sendContractXnetMessage{value: amount}(xnetCallMsg);

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = xnetCallMsg;

        commitParentFinality(nativeSubnet.gatewayAddr);
        executeTopDownMsgs(msgs, nativeSubnet.id, nativeSubnet.gateway);

        assertEq(address(recipient).balance, amount);
    }

    function testMultiSubnet_Token_CallResultRevertsFromChildToParent() public {
        address caller = address(new MockIpcContractRevert());
        address recipient = address(new MockIpcContractRevert());
        uint256 amount = 3;

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        uint256 balance = 100;

        // Fund an account in the subnet.
        token.transfer(caller, balance);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, balance);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        uint256 fundToSend = 10;

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), fundToSend);

        require(token.balanceOf(address(rootSubnet.gateway.manager())) == fundToSend, "initial balance not correct");

        // a cross network message from parent caller to child recipient
        IpcEnvelope memory crossMsg = TestUtils.newXnetCallMsg(
            IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
            IPCAddress({subnetId: tokenSubnet.id, rawAddress: FvmAddressHelper.from(recipient)}),
            amount,
            0
        );

        // result in child is ActorErr, resultMsg should be a bottom up message
        // from child recipient to the parent caller
        IpcEnvelope memory resultMsg = CrossMsgHelper.createResultMsg(crossMsg, OutcomeType.ActorErr, new bytes(0));
        IpcEnvelope[] memory crossMsgs = new IpcEnvelope[](1);
        crossMsgs[0] = resultMsg;

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway,
            crossMsgs
        );

        // execution should be in the root native subnet, in the parent
        // note that the result msg is sent to the caller in the parent,
        // but the caller is MockIpcContractRevert, which reverts whatever
        // call made to `handleIpcMessage`, we should make sure:
        // 1. The submission of checkpoint is never blocked
        // 2. The fund is locked in the gateway as execution is rejected
        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);

        require(
            token.balanceOf(address(rootSubnet.gateway.manager())) == fundToSend,
            "fund should still be locked in gateway"
        );
        require(token.balanceOf(caller) == balance - fundToSend, "fund should still be locked in gateway");
    }

    function testMultiSubnet_Token_CallFromChildToParent() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContract());
        uint256 amount = 3;

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(address(token), DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 1 ether);

        // Fund an account in the subnet.
        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), 15);

        IPCAddress memory from = IPCAddress({subnetId: tokenSubnet.id, rawAddress: FvmAddressHelper.from(caller)});
        IPCAddress memory to = IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(recipient)});
        bytes4 method = bytes4(0x11223344);
        bytes memory params = bytes("hello");
        IpcEnvelope memory envelope = CrossMsgHelper.createCallMsg(from, to, amount, method, params);

        GatewayMessengerFacet messenger = tokenSubnet.gateway.messenger();
        vm.prank(address(caller));
        messenger.sendContractXnetMessage{value: amount}(envelope);

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            tokenSubnet.id,
            tokenSubnet.gateway
        );

        submitBottomUpCheckpoint(checkpoint, tokenSubnet.subnetActor);

        assertEq(token.balanceOf(recipient), amount);
    }

    function testMultiSubnet_Erc20_SendCrossMessageFromParentToChild() public {
        address caller = address(new MockIpcContract());
        address recipient = address(new MockIpcContract());
        uint256 amount = 3;

        token.transfer(caller, 100);
        vm.prank(caller);
        token.approve(rootSubnet.gatewayAddr, 100);

        vm.deal(tokenSubnet.subnetActorAddr, DEFAULT_COLLATERAL_AMOUNT);
        vm.deal(caller, 3);

        vm.prank(tokenSubnet.subnetActorAddr);
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, tokenSubnet.subnetActorAddr, rootSubnet.gateway);

        vm.prank(caller);
        rootSubnet.gateway.manager().fundWithToken(tokenSubnet.id, FvmAddressHelper.from(address(caller)), 15);

        IpcEnvelope memory xnetCallMsg = TestUtils.newXnetCallMsg(
            IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
            IPCAddress({subnetId: tokenSubnet.id, rawAddress: FvmAddressHelper.from(recipient)}),
            amount,
            0
        );

        IpcEnvelope memory committedEvent = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: rootSubnet.id, rawAddress: FvmAddressHelper.from(caller)}),
            to: xnetCallMsg.to,
            value: xnetCallMsg.value,
            message: xnetCallMsg.message,
            nonce: 1
        });

        vm.prank(address(caller));
        vm.expectEmit(true, true, true, true, rootSubnet.gatewayAddr);
        emit LibGateway.NewTopDownMessage({subnet: tokenSubnet.subnetActorAddr, message: committedEvent});
        rootSubnet.gateway.messenger().sendContractXnetMessage{value: amount}(xnetCallMsg);

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = xnetCallMsg;

        commitParentFinality(tokenSubnet.gatewayAddr);
        executeTopDownMsgs(msgs, tokenSubnet.id, tokenSubnet.gateway);

        assertEq(address(recipient).balance, amount);
    }

    function commitParentFinality(address gateway) internal {
        vm.roll(10);
        ParentFinality memory finality = ParentFinality({height: block.number, blockHash: bytes32(0)});

        TopDownFinalityFacet gwTopDownFinalityFacet = TopDownFinalityFacet(address(gateway));

        vm.prank(FilAddress.SYSTEM_ACTOR);
        gwTopDownFinalityFacet.commitParentFinality(finality);
    }

    function executeTopDownMsgs(IpcEnvelope[] memory msgs, SubnetID memory subnet, GatewayDiamond gw) internal {
        XnetMessagingFacet messenger = gw.xnetMessenger();

        uint256 minted_tokens;

        for (uint256 i; i < msgs.length; ) {
            minted_tokens += msgs[i].value;
            unchecked {
                ++i;
            }
        }
        console.log("minted tokens in executed top-downs: %d", minted_tokens);

        // The implementation of the function in fendermint is in
        // https://github.com/consensus-shipyard/ipc/blob/main/fendermint/vm/interpreter/src/fvm/topdown.rs#L43

        // This emulates minting tokens.
        vm.deal(address(gw), minted_tokens);

        // TODO: how to emulate increase of circulation supply?

        vm.prank(FilAddress.SYSTEM_ACTOR);
        messenger.applyCrossMessages(msgs);
    }

    function executeTopDownMsgsRevert(IpcEnvelope[] memory msgs, SubnetID memory subnet, GatewayDiamond gw) internal {
        vm.expectRevert();
        executeTopDownMsgs(msgs, subnet, gw);
    }

    function callCreateBottomUpCheckpointFromChildSubnet(
        SubnetID memory subnet,
        GatewayDiamond gw
    ) internal returns (BottomUpCheckpoint memory checkpoint) {
        uint256 e = getNextEpoch(block.number, DEFAULT_CHECKPOINT_PERIOD);

        GatewayGetterFacet getter = gw.getter();
        CheckpointingFacet checkpointer = gw.checkpointer();

        BottomUpMsgBatch memory batch = getter.bottomUpMsgBatch(e);
        require(batch.msgs.length == 1, "batch length incorrect");

        (, address[] memory addrs, uint256[] memory weights) = TestUtils.getFourValidators(vm);

        (bytes32 membershipRoot, ) = MerkleTreeHelper.createMerkleProofsForValidators(addrs, weights);

        checkpoint = BottomUpCheckpoint({
            subnetID: subnet,
            blockHeight: batch.blockHeight,
            blockHash: keccak256("block1"),
            nextConfigurationNumber: 0,
            msgs: batch.msgs
        });

        vm.startPrank(FilAddress.SYSTEM_ACTOR);
        checkpointer.createBottomUpCheckpoint(checkpoint, membershipRoot, weights[0] + weights[1] + weights[2]);
        vm.stopPrank();

        return checkpoint;
    }

    function callCreateBottomUpCheckpointFromChildSubnet(
        SubnetID memory subnet,
        GatewayDiamond gw,
        IpcEnvelope[] memory msgs
    ) internal returns (BottomUpCheckpoint memory checkpoint) {
        uint256 e = getNextEpoch(block.number, DEFAULT_CHECKPOINT_PERIOD);

        GatewayGetterFacet getter = gw.getter();
        CheckpointingFacet checkpointer = gw.checkpointer();

        (, address[] memory addrs, uint256[] memory weights) = TestUtils.getFourValidators(vm);

        (bytes32 membershipRoot, ) = MerkleTreeHelper.createMerkleProofsForValidators(addrs, weights);

        checkpoint = BottomUpCheckpoint({
            subnetID: subnet,
            blockHeight: e,
            blockHash: keccak256("block1"),
            nextConfigurationNumber: 0,
            msgs: msgs
        });

        vm.startPrank(FilAddress.SYSTEM_ACTOR);
        checkpointer.createBottomUpCheckpoint(checkpoint, membershipRoot, weights[0] + weights[1] + weights[2]);
        vm.stopPrank();

        return checkpoint;
    }

    function submitBottomUpCheckpoint(BottomUpCheckpoint memory checkpoint, SubnetActorDiamond sa) internal {
        (uint256[] memory parentKeys, address[] memory parentValidators, ) = TestUtils.getThreeValidators(vm);
        bytes[] memory parentPubKeys = new bytes[](3);
        bytes[] memory parentSignatures = new bytes[](3);

        SubnetActorManagerFacet manager = sa.manager();

        for (uint256 i = 0; i < 3; i++) {
            vm.deal(parentValidators[i], 10 gwei);
            parentPubKeys[i] = TestUtils.deriveValidatorPubKeyBytes(parentKeys[i]);
            vm.prank(parentValidators[i]);
            manager.join{value: 10}(parentPubKeys[i]);
        }

        bytes32 hash = keccak256(abi.encode(checkpoint));

        for (uint256 i = 0; i < 3; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(parentKeys[i], hash);
            parentSignatures[i] = abi.encodePacked(r, s, v);
        }

        SubnetActorCheckpointingFacet checkpointer = sa.checkpointer();

        vm.startPrank(address(sa));
        checkpointer.submitCheckpoint(checkpoint, parentValidators, parentSignatures);
        vm.stopPrank();
    }

    function submitBottomUpCheckpointRevert(BottomUpCheckpoint memory checkpoint, SubnetActorDiamond sa) internal {
        vm.expectRevert();
        submitBottomUpCheckpoint(checkpoint, sa);
    }

    function getNextEpoch(uint256 blockNumber, uint256 checkPeriod) internal pure returns (uint256) {
        return ((uint64(blockNumber) / checkPeriod) + 1) * checkPeriod;
    }

    function printActors() internal view {
        console.log("root gateway: %s", rootSubnet.gatewayAddr);
        console.log("root actor: %s", rootSubnet.id.getActor());
        console.log("root native subnet actor: %s", (nativeSubnet.subnetActorAddr));
        console.log("root token subnet actor: %s", (tokenSubnet.subnetActorAddr));
        console.log("root name: %s", rootSubnet.id.toString());
        console.log("native subnet name: %s", nativeSubnet.id.toString());
        console.log("token subnet name: %s", tokenSubnet.id.toString());
        console.log("native subnet getActor(): %s", address(nativeSubnet.id.getActor()));
        console.log("native subnet gateway(): %s", nativeSubnet.gatewayAddr);
    }

    function printEnvelope(IpcEnvelope memory envelope) internal view {
        console.log("from %s:", envelope.from.subnetId.toString());
        console.log("to %s:", envelope.to.subnetId.toString());
    }
}
