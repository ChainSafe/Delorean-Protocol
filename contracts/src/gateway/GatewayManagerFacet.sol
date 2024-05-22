// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {GatewayActorModifiers} from "../lib/LibGatewayActorStorage.sol";
import {SubnetActorGetterFacet} from "../subnet/SubnetActorGetterFacet.sol";
import {BURNT_FUNDS_ACTOR} from "../constants/Constants.sol";
import {IpcEnvelope} from "../structs/CrossNet.sol";
import {FvmAddress} from "../structs/FvmAddress.sol";
import {SubnetID, Subnet, SupplySource} from "../structs/Subnet.sol";
import {Membership, SupplyKind} from "../structs/Subnet.sol";
import {AlreadyRegisteredSubnet, CannotReleaseZero, MethodNotAllowed, NotEnoughFunds, NotEnoughFundsToRelease, NotEnoughCollateral, NotEmptySubnetCircSupply, NotRegisteredSubnet, InvalidXnetMessage, InvalidXnetMessageReason} from "../errors/IPCErrors.sol";
import {LibGateway} from "../lib/LibGateway.sol";
import {SubnetIDHelper} from "../lib/SubnetIDHelper.sol";
import {CrossMsgHelper} from "../lib/CrossMsgHelper.sol";
import {FilAddress} from "fevmate/utils/FilAddress.sol";
import {ReentrancyGuard} from "../lib/LibReentrancyGuard.sol";
import {IERC20} from "openzeppelin-contracts/token/ERC20/IERC20.sol";
import {Address} from "openzeppelin-contracts/utils/Address.sol";
import {SupplySourceHelper} from "../lib/SupplySourceHelper.sol";
import {EnumerableSet} from "openzeppelin-contracts/utils/structs/EnumerableSet.sol";

string constant ERR_CHILD_SUBNET_NOT_ALLOWED = "Subnet does not allow child subnets";

contract GatewayManagerFacet is GatewayActorModifiers, ReentrancyGuard {
    using FilAddress for address payable;
    using SubnetIDHelper for SubnetID;
    using SupplySourceHelper for SupplySource;
    using EnumerableSet for EnumerableSet.Bytes32Set;

    /// @notice register a subnet in the gateway. It is called by a subnet when it reaches the threshold stake
    /// @dev The subnet can optionally pass a genesis circulating supply that would be pre-allocated in the
    /// subnet from genesis (without having to wait for the subnet to be spawned to propagate the funds).
    function register(uint256 genesisCircSupply) external payable {
        // If L2+ support is not enabled, only allow the registration of new
        // subnets in the root
        if (s.networkName.route.length + 1 >= s.maxTreeDepth) {
            revert MethodNotAllowed(ERR_CHILD_SUBNET_NOT_ALLOWED);
        }

        if (msg.value < genesisCircSupply) {
            revert NotEnoughFunds();
        }
        uint256 collateral = msg.value - genesisCircSupply;
        SubnetID memory subnetId = s.networkName.createSubnetId(msg.sender);

        (bool registered, Subnet storage subnet) = LibGateway.getSubnet(subnetId);
        if (registered) {
            revert AlreadyRegisteredSubnet();
        }

        subnet.id = subnetId;
        subnet.stake = collateral;
        subnet.genesisEpoch = block.number;
        subnet.circSupply = genesisCircSupply;

        s.subnetKeys.add(subnetId.toHash());
        s.totalSubnets += 1;
    }

    /// @notice addStake - add collateral for an existing subnet
    function addStake() external payable {
        if (msg.value == 0) {
            revert NotEnoughFunds();
        }

        (bool registered, Subnet storage subnet) = LibGateway.getSubnet(msg.sender);

        if (!registered) {
            revert NotRegisteredSubnet();
        }

        subnet.stake += msg.value;
    }

    /// @notice release collateral for an existing subnet.
    /// @dev it can be used to release the stake or reward of the validator.
    /// @param amount The amount of stake to be released.
    function releaseStake(uint256 amount) external nonReentrant {
        if (amount == 0) {
            revert CannotReleaseZero();
        }

        (bool registered, Subnet storage subnet) = LibGateway.getSubnet(msg.sender);

        if (!registered) {
            revert NotRegisteredSubnet();
        }
        if (subnet.stake < amount) {
            revert NotEnoughFundsToRelease();
        }

        subnet.stake -= amount;

        payable(subnet.id.getActor()).sendValue(amount);
    }

    /// @notice kill an existing subnet.
    /// @dev The subnet's balance must be empty.
    function kill() external {
        (bool registered, Subnet storage subnet) = LibGateway.getSubnet(msg.sender);

        if (!registered) {
            revert NotRegisteredSubnet();
        }
        // gas-opt: original check: subnet.circSupply > 0
        if (subnet.circSupply != 0) {
            revert NotEmptySubnetCircSupply();
        }

        uint256 stake = subnet.stake;
        bytes32 id = subnet.id.toHash();

        s.totalSubnets -= 1;
        delete s.subnets[id];

        s.subnetKeys.remove(id);

        payable(msg.sender).sendValue(stake);
    }

    /// @notice credits the received value to the specified address in the specified child subnet.
    ///
    /// @dev There may be an associated fee that gets distributed to validators in the subnet. Currently this fee is zero,
    ///     i.e. funding a subnet is free.
    ///
    /// @param subnetId: the destination subnet for the funds.
    /// @param to: the address to which to credit funds in the destination subnet.
    function fund(SubnetID calldata subnetId, FvmAddress calldata to) external payable {
        if (msg.value == 0) {
            // prevent spamming if there's no value to fund.
            revert InvalidXnetMessage(InvalidXnetMessageReason.Value);
        }
        // slither-disable-next-line unused-return
        (bool registered, ) = LibGateway.getSubnet(subnetId);
        if (!registered) {
            revert NotRegisteredSubnet();
        }

        // Validate that the supply strategy is native.
        SupplySource memory supplySource = SubnetActorGetterFacet(subnetId.getActor()).supplySource();
        supplySource.expect(SupplyKind.Native);

        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg({
            subnet: subnetId,
            signer: msg.sender,
            to: to,
            value: msg.value
        });

        // commit top-down message.
        LibGateway.commitTopDownMsg(crossMsg);
    }

    /// @notice Sends funds to a specified subnet receiver using ERC20 tokens.
    /// @dev This function locks the amount of ERC20 tokens into custody and then mints the supply in the specified subnet.
    ///     It checks if the subnet's supply strategy is ERC20 and if not, the operation is reverted.
    ///     It allows for free injection of funds into a subnet and is protected against reentrancy.
    /// @param subnetId The ID of the subnet where the funds will be sent to.
    /// @param to The funded address.
    /// @param amount The amount of ERC20 tokens to be sent.
    function fundWithToken(SubnetID calldata subnetId, FvmAddress calldata to, uint256 amount) external nonReentrant {
        if (amount == 0) {
            // prevent spamming if there's no value to fund.
            revert InvalidXnetMessage(InvalidXnetMessageReason.Value);
        }
        // slither-disable-next-line unused-return
        (bool registered, ) = LibGateway.getSubnet(subnetId);
        if (!registered) {
            revert NotRegisteredSubnet();
        }

        // Check that the supply strategy is ERC20.
        // There is no need to check whether the subnet exists. If it doesn't exist, the call to getter will revert.
        // LibGateway.commitTopDownMsg will also revert if the subnet doesn't exist.
        SupplySource memory supplySource = SubnetActorGetterFacet(subnetId.getActor()).supplySource();
        supplySource.expect(SupplyKind.ERC20);

        // Locks a specified amount into custody, adjusting for tokens with transfer fees. This operation
        // accommodates inflationary tokens, potentially reflecting a higher effective locked amount.
        // Operation reverts if the effective transferred amount is zero.
        uint256 transferAmount = supplySource.lock({value: amount});

        // Create the top-down message to mint the supply in the subnet.
        IpcEnvelope memory crossMsg = CrossMsgHelper.createFundMsg({
            subnet: subnetId,
            signer: msg.sender,
            to: to,
            value: transferAmount
        });

        // Commit top-down message.
        LibGateway.commitTopDownMsg(crossMsg);
    }

    /// @notice release() burns the received value locally in subnet and commits a bottom-up message to release the assets in the parent.
    ///         The local supply of a subnet is always the native coin, so this method doesn't have to deal with tokens.
    ///
    /// @param to: the address to which to credit funds in the parent subnet.
    function release(FvmAddress calldata to) external payable {
        if (msg.value == 0) {
            // prevent spamming if there's no value to release.
            revert InvalidXnetMessage(InvalidXnetMessageReason.Value);
        }
        IpcEnvelope memory crossMsg = CrossMsgHelper.createReleaseMsg({
            subnet: s.networkName,
            signer: msg.sender,
            to: to,
            value: msg.value
        });

        LibGateway.commitBottomUpMsg(crossMsg);
        // burn funds that are being released
        payable(BURNT_FUNDS_ACTOR).sendValue(msg.value);
    }
}
