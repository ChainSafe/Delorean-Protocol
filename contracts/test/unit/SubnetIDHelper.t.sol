// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "openzeppelin-contracts/utils/Strings.sol";
import "../../src/lib/SubnetIDHelper.sol";

contract SubnetIDHelperTest is Test {
    using Strings for *;
    using SubnetIDHelper for SubnetID;

    address SUBNET_ONE_ADDRESS;
    address SUBNET_TWO_ADDRESS;
    address SUBNET_THREE_ADDRESS;

    uint64 private constant ROOTNET_CHAINID = 123;
    bytes32 constant EMPTY_SUBNET_ID_HASH = 0x197c6df7c087e8e1cdf2a3b0fa558425f3e2b9c661fe0398a28cb4e1d1ec04c5;
    bytes32 constant ROOT_SUBNET_ID_HASH = 0x06e1ac310c4f4fc0fc8eaf2810408e7fd2b50abedce5894fcfeab35ae3b76263;

    SubnetID EMPTY_SUBNET_ID = SubnetID(0, new address[](0));
    SubnetID ROOT_SUBNET_ID = SubnetID(ROOTNET_CHAINID, new address[](0));

    error NoParentForSubnet();
    error EmptySubnet();

    function setUp() public {
        SUBNET_ONE_ADDRESS = makeAddr("subnet_one"); // 0xb0c7ebf9ce6bfce01fba323a8b98054326032522
        SUBNET_TWO_ADDRESS = makeAddr("subnet_two"); // 0x374b3bb66c3a33e054e804d5ea825a8c2514816a
    }

    function test_GetParentSubnet_Fails_EmptySubnet() public {
        vm.expectRevert(NoParentForSubnet.selector);

        EMPTY_SUBNET_ID.getParentSubnet();
    }

    function test_GetParentSubnet_Fails_NoParent() public {
        address[] memory route = new address[](0);

        SubnetID memory emptySubnet = SubnetID(ROOTNET_CHAINID, route);

        vm.expectRevert(NoParentForSubnet.selector);

        emptySubnet.getParentSubnet();
    }

    function test_GetParentSubnet_Works_ParentRoot() public view {
        address[] memory route = new address[](1);
        route[0] = SUBNET_ONE_ADDRESS;

        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, route);

        require(subnetId.getParentSubnet().toHash() == ROOT_SUBNET_ID.toHash());
    }

    function test_GetParentSubnet_Works_ParentSubnetOne() public view {
        address[] memory route = new address[](2);
        route[0] = SUBNET_ONE_ADDRESS;
        route[1] = SUBNET_TWO_ADDRESS;

        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, route);

        address[] memory expectedRoute = new address[](1);
        expectedRoute[0] = SUBNET_ONE_ADDRESS;

        require(subnetId.getParentSubnet().toHash() == SubnetID(ROOTNET_CHAINID, expectedRoute).toHash());
    }

    function test_CommonParent_Works() public view {
        address[] memory route1 = new address[](2);
        route1[0] = SUBNET_ONE_ADDRESS;
        route1[1] = SUBNET_TWO_ADDRESS;
        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, route1);

        address[] memory route2 = new address[](2);
        route2[0] = SUBNET_ONE_ADDRESS;
        route2[1] = SUBNET_THREE_ADDRESS;
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, route2);

        address[] memory expectedRoute = new address[](1);
        expectedRoute[0] = SUBNET_ONE_ADDRESS;

        require(subnetId1.commonParent(subnetId2).toHash() == SubnetID(ROOTNET_CHAINID, expectedRoute).toHash());
    }

    function test_CommonParent_Works_Root() public view {
        address[] memory route1 = new address[](1);
        route1[0] = SUBNET_ONE_ADDRESS;
        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, route1);

        address[] memory route2 = new address[](1);
        route2[0] = SUBNET_TWO_ADDRESS;
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, route2);

        require(subnetId1.commonParent(subnetId2).toHash() == ROOT_SUBNET_ID_HASH);
    }

    function test_CommonParent_Works_NoCommonParent() public view {
        address[] memory route1 = new address[](2);
        route1[0] = SUBNET_ONE_ADDRESS;
        route1[1] = SUBNET_TWO_ADDRESS;
        SubnetID memory subnetId1 = SubnetID(0, route1);

        address[] memory route2 = new address[](1);
        route2[0] = SUBNET_THREE_ADDRESS;
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, route2);

        require(subnetId1.commonParent(subnetId2).toHash() == EMPTY_SUBNET_ID_HASH);
    }

    function test_Down_Works() public view {
        address[] memory route1 = new address[](0);
        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, route1);

        address[] memory route2 = new address[](1);
        route2[0] = SUBNET_ONE_ADDRESS;
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, route2);

        address[] memory route3 = new address[](2);
        route3[0] = SUBNET_ONE_ADDRESS;
        route3[1] = SUBNET_THREE_ADDRESS;
        SubnetID memory subnetId3 = SubnetID(ROOTNET_CHAINID, route3);

        require(subnetId2.down(subnetId1).equals(subnetId2));
        require(subnetId3.down(subnetId1).equals(subnetId2));
    }

    function test_Down_Works_Subnet2RouteLengthLargerThanSubnet1() public {
        address[] memory route1 = new address[](0);
        address[] memory route2 = new address[](1);
        route2[0] = SUBNET_ONE_ADDRESS;

        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, route1);
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, route2);

        vm.expectRevert(SubnetIDHelper.InvalidRoute.selector);
        subnetId1.down(subnetId2);
    }

    function test_Down_Works_Subnet2RouteLenghtEqualToSubnet1() public {
        address[] memory route1 = new address[](1);
        route1[0] = SUBNET_ONE_ADDRESS;

        address[] memory route2 = new address[](1);
        route2[0] = SUBNET_TWO_ADDRESS;

        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, route1);
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, route2);

        vm.expectRevert(SubnetIDHelper.InvalidRoute.selector);
        subnetId1.down(subnetId2);
    }

    function test_Down_Works_WrongRoot() public {
        SubnetID memory subnetId1 = SubnetID(1, new address[](0));
        SubnetID memory subnetId2 = SubnetID(2, new address[](0));

        vm.expectRevert(SubnetIDHelper.DifferentRootNetwork.selector);
        subnetId1.down(subnetId2);
    }

    function test_Down_Works_CommonRootParent() public view {
        address[] memory subnetRoute1 = new address[](2);
        subnetRoute1[0] = address(101);
        subnetRoute1[1] = address(100);

        address[] memory subnetRoute2 = new address[](1);
        subnetRoute2[0] = address(100);

        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, subnetRoute1);
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, subnetRoute2);

        SubnetID memory subnetId = subnetId1.down(subnetId2);

        require(subnetId.toHash() == ROOT_SUBNET_ID.createSubnetId(subnetRoute1[0]).toHash());
    }

    function test_Down_Works_AllCommon() public pure {
        address[] memory subnetRoute1 = new address[](4);
        subnetRoute1[0] = address(100);
        subnetRoute1[1] = address(101);
        subnetRoute1[2] = address(102);
        subnetRoute1[3] = address(103);

        address[] memory subnetRoute2 = new address[](2);
        subnetRoute2[0] = address(100);
        subnetRoute2[1] = address(101);

        SubnetID memory subnetId1 = SubnetID(ROOTNET_CHAINID, subnetRoute1);
        SubnetID memory subnetId2 = SubnetID(ROOTNET_CHAINID, subnetRoute2);

        SubnetID memory subnetId = subnetId1.down(subnetId2);

        address[] memory expectedRoute = new address[](3);
        expectedRoute[0] = address(100);
        expectedRoute[1] = address(101);
        expectedRoute[2] = address(102);

        require(subnetId.toHash() == SubnetID(ROOTNET_CHAINID, expectedRoute).toHash());
    }

    function test_GetAddress_Works() public pure {
        address[] memory subnetRoute1 = new address[](2);
        subnetRoute1[0] = address(101);
        subnetRoute1[1] = address(100);

        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, subnetRoute1);

        require(subnetId.getAddress() == address(100), "address from subnet id invalid");
    }

    function test_ToString_Works_NoRoutes() public view {
        require(EMPTY_SUBNET_ID.toString().equal("/r0"));
    }

    function test_ToString_Works_Root() public pure {
        address[] memory route = new address[](0);
        require(SubnetID(ROOTNET_CHAINID, route).toString().equal("/r123"));
    }

    function test_ToString_Works_ChildRoute() public view {
        address[] memory route = new address[](2);
        route[0] = SUBNET_ONE_ADDRESS;
        route[1] = SUBNET_TWO_ADDRESS;

        require(
            SubnetID(ROOTNET_CHAINID, route).toString().equal(
                "/r123/0xb0c7ebf9ce6bfce01fba323a8b98054326032522/0x374b3bb66c3a33e054e804d5ea825a8c2514816a"
            )
        );
    }

    function test_ToHash_Works_EmptySubnet() public view {
        require(EMPTY_SUBNET_ID.toHash() == EMPTY_SUBNET_ID_HASH);
    }

    function test_ToHash_Works_NonEmptySubnet() public view {
        address[] memory route = new address[](1);
        route[0] = SUBNET_ONE_ADDRESS;

        SubnetID memory subnetId = SubnetID(ROOTNET_CHAINID, route);

        bytes32 expectedSubnetIdHash = keccak256(abi.encode(subnetId));

        require(subnetId.toHash() == expectedSubnetIdHash);
    }

    function test_CreateSubnetId_Works() public view {
        address[] memory route = new address[](0);

        SubnetID memory subnetId = SubnetID({root: ROOTNET_CHAINID, route: route}).createSubnetId(SUBNET_ONE_ADDRESS);

        address[] memory expectedRoute = new address[](1);
        expectedRoute[0] = SUBNET_ONE_ADDRESS;

        require(subnetId.toHash() == SubnetID({root: ROOTNET_CHAINID, route: expectedRoute}).toHash());
    }

    function test_GetActor_Works_EmptySubnet() public view {
        address emptyActor = EMPTY_SUBNET_ID.getActor();
        require(emptyActor == address(0));
    }

    function test_GetActor_Works_RootSubnet() public pure {
        address[] memory route = new address[](0);

        address emptyActor = SubnetID({root: ROOTNET_CHAINID, route: route}).getActor();
        require(emptyActor == address(0));
    }

    function test_GetActor_Works_EmptyActor() public view {
        address[] memory route = new address[](1);
        route[0] = SUBNET_ONE_ADDRESS;

        address actor = SubnetID({root: ROOTNET_CHAINID, route: route}).getActor();
        require(actor == SUBNET_ONE_ADDRESS);
    }

    function test_IsRoot_Works_EmptySubnet() public view {
        require(EMPTY_SUBNET_ID.isRoot() == false);
    }

    function test_IsRoot_Works_ChildSubnet() public view {
        address[] memory route = new address[](1);
        route[0] = SUBNET_ONE_ADDRESS;

        require(SubnetID({root: ROOTNET_CHAINID, route: route}).isRoot() == false);
    }

    function test_IsRoot_Works_RootSubnet() public pure {
        address[] memory route = new address[](0);

        require(SubnetID({root: ROOTNET_CHAINID, route: route}).isRoot() == true);
    }

    function test_Equals_Works_Empty() public view {
        require(EMPTY_SUBNET_ID.equals(EMPTY_SUBNET_ID) == true);
        require(EMPTY_SUBNET_ID.equals(SubnetID({root: 0, route: new address[](0)})) == true);
        address[] memory route = new address[](0);
        require(EMPTY_SUBNET_ID.equals(SubnetID({root: 0, route: route})) == true);
    }

    function test_Equals_Works_NonEmpty() public view {
        address[] memory route = new address[](1);
        route[0] = SUBNET_ONE_ADDRESS;

        address[] memory route2 = new address[](1);
        route2[0] = SUBNET_TWO_ADDRESS;

        require(
            SubnetID({root: ROOTNET_CHAINID, route: route}).equals(SubnetID({root: ROOTNET_CHAINID, route: route})) ==
                true
        );
        require(
            SubnetID({root: ROOTNET_CHAINID, route: route}).equals(SubnetID({root: ROOTNET_CHAINID, route: route2})) ==
                false
        );
    }

    function test_Equals_Works_RootNotSame() public view {
        require(EMPTY_SUBNET_ID.equals(SubnetID({root: ROOTNET_CHAINID, route: new address[](0)})) == false);
    }

    function test_IsEmpty_Works_Empty() public view {
        require(EMPTY_SUBNET_ID.isEmpty());
    }

    function test_IsEmpty_Works_NonEmpty() public view {
        require(ROOT_SUBNET_ID.isEmpty() == false);
    }
}
