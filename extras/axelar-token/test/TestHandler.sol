// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/IpcTokenHandler.sol";
import "./DummyERC20.sol";
import { FvmAddressHelper } from "@ipc/src/lib/FvmAddressHelper.sol";

contract TestHandler is Test {
    using FvmAddressHelper for address;

    function test_handler_Ok() public {
        address axelarIts = vm.addr(1);
        address ipcGateway = vm.addr(2);
        address owner = vm.addr(3);
        DummyERC20 token = new DummyERC20("Test token", "TST", 10000);

        IpcTokenHandler handler = new IpcTokenHandler({
            axelarIts: axelarIts,
            ipcGateway: ipcGateway,
            admin: owner
        });

        address[] memory route = new address[](1);
        route[0] = 0x2a3eF0F414c626e51AFA2F29f3F7Be7a45C6DB09;
        SubnetID memory subnet = SubnetID({ root: 314159, route: route });

        address recipient = 0x6B505cdCCCA34aE8eea5D382aBaD40d2AfEa74ad;

        bytes memory params = abi.encode(subnet, recipient);

        token.transfer(address(handler), 1);
        vm.startPrank(axelarIts);

        vm.mockCall(
            address(ipcGateway),
            abi.encodeWithSelector(TokenFundedGateway.fundWithToken.selector, subnet, recipient.from(), 1),
            abi.encode("")
        );
        handler.executeWithInterchainToken(bytes32(""), "", "", params, bytes32(""), address(token), 1);

        // the allowance of the gateway is still 1, because the call to fundWithToken was mocked and did not actually expend the allowance
        // this is not what would happen in reality, but the assert gives us extra insight
        require(token.allowance(address(handler), ipcGateway) == 1);
    }

    function test_handler_failGateway() public {
        address axelarIts = vm.addr(1);
        address ipcGateway = vm.addr(2);
        address owner = vm.addr(3);
        DummyERC20 token = new DummyERC20("Test token", "TST", 10000);

        IpcTokenHandler handler = new IpcTokenHandler({
            axelarIts: axelarIts,
            ipcGateway: ipcGateway,
            admin: owner
        });

        address[] memory route = new address[](1);
        route[0] = 0x2a3eF0F414c626e51AFA2F29f3F7Be7a45C6DB09;
        SubnetID memory subnet = SubnetID({ root: 314159, route: route });

        address recipient = 0x6B505cdCCCA34aE8eea5D382aBaD40d2AfEa74ad;

        bytes memory params = abi.encode(subnet, recipient);

        token.transfer(address(handler), 1);
        vm.startPrank(axelarIts);

        vm.expectEmit();
        emit IERC20.Approval(address(handler), address(ipcGateway), 1);
        emit IERC20.Approval(address(handler), address(ipcGateway), 0);
        emit IERC20.Approval(address(handler), address(owner), 1);
        emit IpcTokenHandler.FundingFailed(subnet, recipient, 1);

        vm.mockCallRevert(
            address(ipcGateway),
            abi.encodeWithSelector(TokenFundedGateway.fundWithToken.selector, subnet, recipient.from(), 1),
            abi.encode("ERROR")
        );
        handler.executeWithInterchainToken(bytes32(""), "", "", params, bytes32(""), address(token), 1);

        // the allowance was accrued to the owner
        require(token.allowance(address(handler), ipcGateway) == 0);
        require(token.allowance(address(handler), owner) == 1);
    }

    function test_handler_fail_unexpected() public {
        address axelarIts = vm.addr(1);
        address ipcGateway = vm.addr(2);
        address owner = vm.addr(3);
        DummyERC20 token = new DummyERC20("Test token", "TST", 10000);

        IpcTokenHandler handler = new IpcTokenHandler({
            axelarIts: axelarIts,
            ipcGateway: ipcGateway,
            admin: owner
        });
        
        // garbage
        bytes memory params = abi.encode(1);

        token.transfer(address(handler), 4200);
        vm.startPrank(axelarIts);

        // will revert due to garbage.
        vm.expectRevert();
        handler.executeWithInterchainToken(bytes32(""), "", "", params, bytes32(""), address(token), 4200);

        // let's ensure we can recover the tokens
        require(token.allowance(address(handler), owner) == 0);

        // this should revert when called by the previous pranked account (non-owner).
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, axelarIts));
        handler.adminTokenIncreaseAllowance(address(token), 4200);

        // now act like the owner.
        vm.startPrank(owner);

        handler.adminTokenIncreaseAllowance(address(token), 4200);
        require(token.allowance(address(handler), owner) == 4200);

        token.transferFrom(address(handler), owner, 4200);
        require(token.allowance(address(handler), owner) == 0);
        require(token.balanceOf(owner) == 4200);
    }

}