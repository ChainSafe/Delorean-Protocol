// SPDX-License-Identifier: MIT
pragma solidity 0.8.23;

import {IERC20Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC20/IERC20Upgradeable.sol";

import {IntegrationTestBase, RootSubnetDefinition, TestSubnetDefinition} from "@ipc/test/IntegrationTestBase.sol";
import {ERC20PresetFixedSupply} from "@ipc/test/helpers/ERC20PresetFixedSupply.sol";
import {TestUtils} from "@ipc/test/helpers/TestUtils.sol";
import {MerkleTreeHelper} from "@ipc/test/helpers/MerkleTreeHelper.sol";
import {GatewayFacetsHelper} from "@ipc/test/helpers/GatewayFacetsHelper.sol";
import {SubnetActorFacetsHelper} from "@ipc/test/helpers/SubnetActorFacetsHelper.sol";
import {LinkedTokenController} from "../src/LinkedTokenController.sol";
import {LinkedTokenReplica} from "../src/LinkedTokenReplica.sol";

import {LinkedTokenControllerV2} from "../src/v2/LinkedTokenControllerV2.sol";
import {LinkedTokenControllerV2Extension} from "./LinkedTokenControllerV2Extension.sol";
import {LinkedTokenReplicaV2Extension} from "./LinkedTokenReplicaV2Extension.sol";
import {LinkedTokenReplicaV2} from "../src/v2/LinkedTokenReplicaV2.sol";
import {USDCTest} from "../src/USDCTest.sol";

import {SubnetID, Subnet, IPCAddress, Validator} from "@ipc/src/structs/Subnet.sol";
import {SubnetActorDiamond} from "@ipc/src/SubnetActorDiamond.sol";
import {GatewayDiamond} from "@ipc/src/GatewayDiamond.sol";
import {TopDownFinalityFacet} from "@ipc/src/gateway/router/TopDownFinalityFacet.sol";
import {XnetMessagingFacet} from "@ipc/src/gateway/router/XnetMessagingFacet.sol";
import {SubnetActorManagerFacet} from "@ipc/src/subnet/SubnetActorManagerFacet.sol";
import {GatewayGetterFacet} from "@ipc/src/gateway/GatewayGetterFacet.sol";
import {SubnetActorCheckpointingFacet} from "@ipc/src/subnet/SubnetActorCheckpointingFacet.sol";
import {CheckpointingFacet} from "@ipc/src/gateway/router/CheckpointingFacet.sol";
import {FvmAddressHelper} from "@ipc/src/lib/FvmAddressHelper.sol";
import {IpcEnvelope, BottomUpMsgBatch, BottomUpCheckpoint, ParentFinality, IpcMsgKind, ResultMsg, CallMsg} from "@ipc/src/structs/CrossNet.sol";
import {SubnetIDHelper} from "@ipc/src/lib/SubnetIDHelper.sol";
import {IERC20} from "openzeppelin-contracts/token/ERC20/IERC20.sol";
import {CrossMsgHelper} from "@ipc/src/lib/CrossMsgHelper.sol";
import {IIpcHandler} from "@ipc/sdk/interfaces/IIpcHandler.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import "forge-std/console.sol";

import "openzeppelin-contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

string constant REPLICA_TOKEN_NAME = "USDCTestReplica";
string constant REPLICA_TOKEN_SYMBOL = "USDCtR";
uint8 constant REPLICA_TOKEN_DECIMALS = 6;

contract MultiSubnetTest is IntegrationTestBase {
    using SubnetIDHelper for SubnetID;
    using GatewayFacetsHelper for GatewayDiamond;
    using SubnetActorFacetsHelper for SubnetActorDiamond;
    // @dev This test verifies that USDC bridge connects correctly
    // a contract from native subnet with a contract in token subnet through the rootnet.
    using CrossMsgHelper for IpcEnvelope;

    LinkedTokenReplica ipcTokenReplica;
    LinkedTokenController ipcTokenController;

    RootSubnetDefinition public rootSubnet;
    TestSubnetDefinition public nativeSubnet;
    TestSubnetDefinition public tokenSubnet;

    SubnetActorDiamond rootTokenSubnetActor;
    SubnetActorDiamond rootNativeSubnetActor;
    GatewayDiamond rootGateway;
    GatewayDiamond nativeSubnetGateway;
    SubnetID rootSubnetName;
    SubnetID nativeSubnetName;

    IERC20 public token;
    USDCTest testUSDC;

    IpcEnvelope expected;

    address holder;
    address recipient;
    address owner;
    uint256 transferAmount;
    uint256 holderTotalAmount;

    function setUp() public override {
        holder = vm.addr(100);
        recipient = vm.addr(200);
        owner = address(this);
        transferAmount = 300;
        holderTotalAmount = 1000;

        token = new ERC20PresetFixedSupply("TestToken", "TEST", 1_000_000, address(this));

        rootSubnetName = SubnetID({root: ROOTNET_CHAINID, route: new address[](0)});
        require(rootSubnetName.isRoot(), "not root");

        rootGateway = createGatewayDiamond(gatewayParams(rootSubnetName));

        rootNativeSubnetActor = createSubnetActor(defaultSubnetActorParamsWith(address(rootGateway), rootSubnetName));

        rootTokenSubnetActor = createSubnetActor(
            defaultSubnetActorParamsWith(address(rootGateway), rootSubnetName, address(token))
        );

        address[] memory tokenSubnetPath = new address[](1);
        tokenSubnetPath[0] = address(rootTokenSubnetActor);
        SubnetID memory tokenSubnetName = SubnetID({root: ROOTNET_CHAINID, route: tokenSubnetPath});
        GatewayDiamond tokenSubnetGateway = createGatewayDiamond(gatewayParams(tokenSubnetName));

        address[] memory nativeSubnetPath = new address[](1);
        nativeSubnetPath[0] = address(rootNativeSubnetActor);
        nativeSubnetName = SubnetID({root: ROOTNET_CHAINID, route: nativeSubnetPath});
        nativeSubnetGateway = createGatewayDiamond(gatewayParams(nativeSubnetName));

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

        tokenSubnet = TestSubnetDefinition({
            gateway: tokenSubnetGateway,
            gatewayAddr: address(tokenSubnetGateway),
            id: tokenSubnetName,
            subnetActor: rootTokenSubnetActor,
            subnetActorAddr: address(rootTokenSubnetActor),
            path: tokenSubnetPath
        });

        vm.deal(address(rootTokenSubnetActor), DEFAULT_COLLATERAL_AMOUNT);
        vm.prank(address(rootTokenSubnetActor));
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, address(rootTokenSubnetActor), rootGateway);

        vm.deal(address(rootNativeSubnetActor), DEFAULT_COLLATERAL_AMOUNT);
        vm.prank(address(rootNativeSubnetActor));
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, address(rootNativeSubnetActor), rootGateway);

        testUSDC = new USDCTest();

        //set up controller with proxy
        LinkedTokenController initialControllerImplementation = new LinkedTokenController();

        bytes memory initCallController = abi.encodeCall(
            LinkedTokenController.initialize,
            (address(rootGateway), address(testUSDC), nativeSubnetName, address(0))
        );
        TransparentUpgradeableProxy transparentProxyController = new TransparentUpgradeableProxy(
            address(initialControllerImplementation),
            address(this),
            initCallController
        );

        ipcTokenController = LinkedTokenController(address(transparentProxyController));

        //set up replica with proxy
        LinkedTokenReplica initialReplicaImplementation = new LinkedTokenReplica();

        bytes memory initCallReplica = abi.encodeCall(
            LinkedTokenReplica.initialize,
            (
                address(nativeSubnetGateway),
                address(testUSDC),
                rootSubnetName,
                address(0),
                REPLICA_TOKEN_NAME,
                REPLICA_TOKEN_SYMBOL,
                REPLICA_TOKEN_DECIMALS
            )
        );
        TransparentUpgradeableProxy transparentProxyReplica = new TransparentUpgradeableProxy(
            address(initialReplicaImplementation),
            address(this),
            initCallReplica
        );

        ipcTokenReplica = LinkedTokenReplica(address(transparentProxyReplica));

        // set linked contract controller & replica
        ipcTokenController.setLinkedContract(address(ipcTokenReplica));
        ipcTokenReplica.setLinkedContract(address(ipcTokenController));
    }

    function testMultiSubnet_Native_FundFromParentToChild_USDCBridge() public {
        _testLinkedTokenBridge();
    }

    function testMultiSubnet_Native_FundFromParentToChild_USDCBridge_UpgradeFirst() public {
        upgradeController();
        upgradeReplica();
        _testLinkedTokenBridge();
    }

    function testMultiSubnet_Native_FundFromParentToChild_USDCBridge_UpgradeReplica() public {
        _testTransferTopDown();
        upgradeController();
        upgradeReplica();
        _testTransferBottomUp();
    }

    function upgradeController() public {
        bytes memory initCallController = abi.encodeCall(
            LinkedTokenControllerV2.reinitialize,
            (address(rootGateway), address(testUSDC), nativeSubnetName, address(ipcTokenReplica))
        );
        LinkedTokenControllerV2Extension newControllerImplementation = new LinkedTokenControllerV2Extension();
        ipcTokenController.upgradeToAndCall(address(newControllerImplementation), initCallController);

        require(
            LinkedTokenControllerV2Extension(address(ipcTokenController)).newFunctionReturns7() == 7,
            "controller upgrade failed"
        );
    }

    function upgradeReplica() public {
        bytes memory initCallReplica = abi.encodeCall(
            LinkedTokenReplicaV2.reinitialize,
            (
                address(nativeSubnetGateway),
                address(testUSDC),
                rootSubnetName,
                address(ipcTokenController),
                REPLICA_TOKEN_NAME,
                REPLICA_TOKEN_SYMBOL,
                REPLICA_TOKEN_DECIMALS
            )
        );
        LinkedTokenReplicaV2Extension newReplicaImplementation = new LinkedTokenReplicaV2Extension();
        ipcTokenReplica.upgradeToAndCall(address(newReplicaImplementation), initCallReplica);
        require(
            LinkedTokenReplicaV2Extension(address(ipcTokenReplica)).newFunctionReturns8() == 8,
            "replica upgrade failed"
        );
    }

    function _testLinkedTokenBridge() public {
        _testTransferTopDown();
        _testTransferBottomUp();
    }

    function _testTransferTopDown() public {
        console.log("--------------- transfer and mint (top-down) ---------------");

        testUSDC.mint(100_000);
        testUSDC.transfer(holder, holderTotalAmount);

        require(testUSDC.owner() == owner, "unexpected owner");
        require(testUSDC.balanceOf(holder) == holderTotalAmount, "unexpected balance");

        vm.prank(holder);
        testUSDC.approve(address(ipcTokenController), transferAmount);

        console.log("mock usdc contract: %s", address(testUSDC));
        console.log("mock usdc owner: %s", owner);
        console.log("mock usdc holder: %s", address(holder));
        console.log("ipcTokenController: %s", address(ipcTokenController));
        console.log(
            "controller allowance for holder: %d",
            testUSDC.allowance(address(holder), address(ipcTokenController))
        );

        vm.prank(address(holder));
        IpcEnvelope memory lockAndTransferEnvelope = ipcTokenController.lockAndTransferWithReturn(
            recipient,
            transferAmount
        );

        // Check that the message is in unconfirmedTransfers
        (address receiptSender, uint256 receiptValue) = ipcTokenController.getUnconfirmedTransfer(
            lockAndTransferEnvelope.toHash()
        );
        require(receiptSender == address(holder), "Transfer sender incorrect in unconfirmedTransfers");
        require(receiptValue == transferAmount, "Transfer amount incorrect in unconfirmedTransfers");

        //confirm that token replica only accept calls to Ipc from the gateway
        vm.prank(owner);
        vm.expectRevert(IIpcHandler.CallerIsNotGateway.selector);
        ipcTokenReplica.handleIpcMessage(expected);

        // the message the root gateway's postbox is being executed in the token subnet's gateway

        expected = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({
                subnetId: rootSubnetName,
                rawAddress: FvmAddressHelper.from(address(ipcTokenController))
            }),
            to: lockAndTransferEnvelope.to,
            value: 0,
            message: lockAndTransferEnvelope.message,
            nonce: 0 // nonce will be updated by LibGateway.commitCrossMessage
        });

        IpcEnvelope[] memory msgs = new IpcEnvelope[](1);
        msgs[0] = expected;
        executeTopDownMsgs(msgs, nativeSubnetName, nativeSubnetGateway);

        //ensure that tokens are delivered on subnet
        require(
            IERC20Upgradeable(ipcTokenReplica).balanceOf(recipient) == transferAmount,
            "incorrect proxy token balance"
        );
    }

    function _testTransferBottomUp() public {
        console.log("--------------- withdraw token (bottom-up)---------------");

        // ensure that USDC holder has initial balance minus tokens previously sent amount of tokens in the root chain
        require(
            holderTotalAmount - transferAmount == testUSDC.balanceOf(holder),
            "unexpected holder balance in withdraw flow"
        );

        vm.prank(recipient);
        expected = ipcTokenReplica.linkedTransfer(holder, transferAmount);

        // check that the message is in unconfirmedTransfers
        (address receiptSender, uint256 receiptValue) = ipcTokenReplica.getUnconfirmedTransfer(expected.toHash());
        require(receiptSender == recipient, "Transfer sender incorrect in unconfirmedTransfers");
        require(receiptValue == transferAmount, "Transfer amount incorrect in unconfirmedTransfers");

        console.log("Begin bottom up checkpoint");

        BottomUpCheckpoint memory checkpoint = callCreateBottomUpCheckpointFromChildSubnet(
            nativeSubnetName,
            nativeSubnetGateway
        );
        submitBottomUpCheckpoint(checkpoint, rootNativeSubnetActor);

        //ensure that usdc tokens are returned on root net
        require(holderTotalAmount == testUSDC.balanceOf(holder), "unexpected holder balance after withdrawal");
        //ensure that the tokens in the subnet are minted and the token bridge and the usdc holder does not own any
        require(0 == ipcTokenReplica.balanceOf(holder), "unexpected holder balance in ipcTokenReplica");
        require(
            0 == ipcTokenReplica.balanceOf(address(ipcTokenReplica)),
            "unexpected ipcTokenReplica balance in ipcTokenReplica"
        );
    }

    function executeTopDownMsgs(IpcEnvelope[] memory msgs, SubnetID memory _subnet, GatewayDiamond gw) internal {
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

    function callCreateBottomUpCheckpointFromChildSubnet(
        SubnetID memory subnet,
        GatewayDiamond gw
    ) internal returns (BottomUpCheckpoint memory checkpoint) {
        uint256 e = getNextEpoch(block.number, DEFAULT_CHECKPOINT_PERIOD);

        GatewayGetterFacet getter = gw.getter();
        CheckpointingFacet checkpointer = gw.checkpointer();

        BottomUpMsgBatch memory batch = getter.bottomUpMsgBatch(e);

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

    function getNextEpoch(uint256 blockNumber, uint256 checkPeriod) internal pure returns (uint256) {
        return ((uint64(blockNumber) / checkPeriod) + 1) * checkPeriod;
    }
}
