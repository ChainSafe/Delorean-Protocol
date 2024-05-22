// SPDX-License-Identifier: MIT
pragma solidity 0.8.23;

import "forge-std/Test.sol";
import "../src/LinkedTokenReplica.sol";
import {IntegrationTestBase} from "@ipc/test/IntegrationTestBase.sol";
import {GatewayDiamond} from "@ipc/src/GatewayDiamond.sol";
import {SubnetIDHelper} from "@ipc/src/lib/SubnetIDHelper.sol";
import {SubnetID, IPCAddress} from "@ipc/src/structs/Subnet.sol";
import {FvmAddressHelper} from "@ipc/src/lib/FvmAddressHelper.sol";
import {FvmAddress} from "@ipc/src/structs/FvmAddress.sol";

import {IpcEnvelope, CallMsg, IpcMsgKind} from "@ipc/src/structs/CrossNet.sol";

import {SubnetActorDiamond} from "@ipc/src/SubnetActorDiamond.sol";
import {LinkedTokenController} from "../src/LinkedTokenController.sol";
//import {InvalidOriginContract, InvalidOriginSubnet} from "@ipc/src/examples/cross-token/IpcCrossTokenErrors.sol";
import {USDCTest} from "../src/USDCTest.sol";
import {InvalidOriginContract, InvalidOriginSubnet} from "../src/LinkedToken.sol";

import "openzeppelin-contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

string constant REPLICA_TOKEN_NAME = "USDCTestReplica";
string constant REPLICA_TOKEN_SYMBOL = "USDCtR";
uint8 constant REPLICA_TOKEN_DECIMALS = 6;

contract LinkedTokenReplicaTest is Test, IntegrationTestBase {
    using SubnetIDHelper for SubnetID;

    LinkedTokenController controller;
    LinkedTokenReplica replica;
    address controllerSubnetUSDC;
    SubnetID controllerSubnet;
    SubnetID replicaSubnetName;
    address gateway;
    GatewayDiamond public rootGateway;
    uint256 transferAmount = 100;

    address[] public nativeSubnetPath;

    SubnetActorDiamond public rootNativeSubnetActor;
    USDCTest public testUSDC;

    function setUp() public override {
        testUSDC = new USDCTest();
        testUSDC.mint(transferAmount);
        controllerSubnetUSDC = address(testUSDC);

        controllerSubnet = SubnetID({root: ROOTNET_CHAINID, route: new address[](0)});
        require(controllerSubnet.isRoot(), "not root");
        rootGateway = createGatewayDiamond(gatewayParams(controllerSubnet));
        gateway = address(rootGateway);
        rootNativeSubnetActor = createSubnetActor(defaultSubnetActorParamsWith(address(rootGateway), controllerSubnet));
        nativeSubnetPath = new address[](1);
        nativeSubnetPath[0] = address(rootNativeSubnetActor);
        replicaSubnetName = SubnetID({root: ROOTNET_CHAINID, route: nativeSubnetPath});

        vm.deal(address(rootNativeSubnetActor), DEFAULT_COLLATERAL_AMOUNT);
        vm.prank(address(rootNativeSubnetActor));
        registerSubnetGW(DEFAULT_COLLATERAL_AMOUNT, address(rootNativeSubnetActor), rootGateway);

        //set up controller with proxy
        LinkedTokenController initialControllerImplementation = new LinkedTokenController();
        TransparentUpgradeableProxy transparentProxyController = new TransparentUpgradeableProxy(
            address(initialControllerImplementation),
            address(this),
            ""
        );
        controller = LinkedTokenController(address(transparentProxyController));

        //set up replica with proxy
        LinkedTokenReplica initialReplicaImplementation = new LinkedTokenReplica();
        TransparentUpgradeableProxy transparentProxyReplica = new TransparentUpgradeableProxy(
            address(initialReplicaImplementation),
            address(this),
            ""
        );
        replica = LinkedTokenReplica(address(transparentProxyReplica));

        // initialize controller & replica

        controller.initialize(gateway, controllerSubnetUSDC, replicaSubnetName, address(replica));
        replica.initialize(
            gateway,
            controllerSubnetUSDC,
            controllerSubnet,
            address(controller),
            REPLICA_TOKEN_NAME,
            REPLICA_TOKEN_SYMBOL,
            REPLICA_TOKEN_DECIMALS
        );

        replica.setLinkedContract(address(controller));
        controller.setLinkedContract(address(replica));
    }

    function testHandleIpcMessageOrigin() public {
        CallMsg memory message = CallMsg({
            method: abi.encodePacked(bytes4(keccak256("receiveAndMint(address,uint256)"))),
            params: abi.encode(address(this), transferAmount)
        });

        IpcEnvelope memory validMsg = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: controllerSubnet, rawAddress: FvmAddressHelper.from(address(controller))}),
            to: IPCAddress({subnetId: replicaSubnetName, rawAddress: FvmAddressHelper.from(address(replica))}),
            value: DEFAULT_CROSS_MSG_FEE,
            nonce: 0,
            message: abi.encode(message)
        });

        IpcEnvelope memory invalidContract = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: controllerSubnet, rawAddress: FvmAddressHelper.from(address(replica))}),
            to: IPCAddress({subnetId: replicaSubnetName, rawAddress: FvmAddressHelper.from(address(replica))}),
            value: DEFAULT_CROSS_MSG_FEE,
            nonce: 0,
            message: abi.encode(message)
        });

        IpcEnvelope memory invalidSubnet = IpcEnvelope({
            kind: IpcMsgKind.Call,
            from: IPCAddress({subnetId: replicaSubnetName, rawAddress: FvmAddressHelper.from(address(replica))}),
            to: IPCAddress({subnetId: replicaSubnetName, rawAddress: FvmAddressHelper.from(address(controller))}),
            value: DEFAULT_CROSS_MSG_FEE,
            nonce: 0,
            message: abi.encode(message)
        });

        vm.expectRevert(InvalidOriginContract.selector);
        replica._validateEnvelope(invalidContract);

        vm.expectRevert(InvalidOriginSubnet.selector);
        replica._validateEnvelope(invalidSubnet);

        replica._validateEnvelope(validMsg);
    }
}
