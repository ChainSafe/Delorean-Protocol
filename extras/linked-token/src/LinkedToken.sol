// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.23;

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {SafeERC20} from "openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";
import {IERC20} from "openzeppelin-contracts/token/ERC20/IERC20.sol";
import {FvmAddressHelper} from "@ipc/src/lib/FvmAddressHelper.sol";
import {FvmAddress} from "@ipc/src/structs/FvmAddress.sol";
import {IpcExchangeUpgradeable} from "@ipc/sdk/IpcContractUpgradeable.sol";
import {IpcEnvelope, ResultMsg, CallMsg, OutcomeType, IpcMsgKind} from "@ipc/src/structs/CrossNet.sol";
import {IPCAddress, SubnetID} from "@ipc/src/structs/Subnet.sol";
import {CrossMsgHelper} from "@ipc/src/lib/CrossMsgHelper.sol";
import {SubnetIDHelper} from "@ipc/src/lib/SubnetIDHelper.sol";
error InvalidOriginContract();
error InvalidOriginSubnet();

/**
 * @title LinkedToken
 * @notice Contract to handle token transfer from L1, lock them and mint on L2.
 */
abstract contract LinkedToken is Initializable, IpcExchangeUpgradeable {
    using SafeERC20 for IERC20;
    using CrossMsgHelper for IpcEnvelope;
    using SubnetIDHelper for SubnetID;
    using FvmAddressHelper for FvmAddress;

    IERC20 public _underlying;
    address public _linkedContract;
    SubnetID public _linkedSubnet;

    mapping(bytes32 => UnconfirmedTransfer) public _unconfirmedTransfers;

    string private constant ERR_ZERO_ADDRESS = "zero address is not allowed";
    string private constant ERR_VALUE_MUST_BE_ZERO = "value must be zero";
    string private constant ERR_AMOUNT_CANNOT_BE_ZERO = "amount cannot be zero";

    error InvalidEnvelope(string reason);
    error TransferRejected(string reason);
    struct UnconfirmedTransfer {
        address sender;
        uint256 value;
    }

    event LinkedTokenInitialized(
        address gateway,
        address indexed underlying,
        SubnetID indexed linkedSubnet,
        address indexed linkedContract
    );

    event LinkedTokensSent(
        address indexed underlying,
        address indexed sender,
        address indexed recipient,
        bytes32 id,
        uint64 nonce,
        uint256 value
    );

    event LinkedTokenReceived(address indexed recipient, uint256 amount);

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /**
     * @dev Initialize function for IpcTokenController
     * @param gateway Address of the gateway for cross-network communication
     * @param underlyingToken Address of the destination contract for minting
     * @param linkedSubnet SubnetID of the destination network
     */
    function __LinkedToken_init(
        address gateway,
        address underlyingToken,
        SubnetID memory linkedSubnet,
        address linkedContract
    ) public onlyInitializing {
        __IpcExchangeUpgradeable_init(gateway);
        _underlying = IERC20(underlyingToken);
        _linkedSubnet = linkedSubnet;
        _linkedContract = linkedContract;

        emit LinkedTokenInitialized({
            gateway: gateway,
            underlying: address(_underlying),
            linkedSubnet: _linkedSubnet,
            linkedContract: _linkedContract
        });
    }

    function setLinkedContract(address linkedContract) external onlyOwner {
        _linkedContract = linkedContract;
    }

    function getLinkedSubnet() public view returns (SubnetID memory) {
        return _linkedSubnet;
    }

    function _captureTokens(address holder, uint256 amount) internal virtual;

    function _releaseTokens(address beneficiary, uint256 amount) internal virtual;

    /**
     * @notice Transfers tokens from L1, locks them, and requests minting on L2.
     * @param receiver Address to receive the minted tokens on L2
     * @param amount Amount of tokens to be transferred and minted
     */
    function linkedTransfer(address receiver, uint256 amount) external returns (IpcEnvelope memory committed) {
        return _linkedTransfer(receiver, amount);
    }

    function _linkedTransfer(address recipient, uint256 amount) internal returns (IpcEnvelope memory committed) {
        _validateInitialized();

        // Validate that the transfer parameters are acceptable.
        _validateTransfer(recipient, amount);

        // Lock or burn, depending on concrete implementation.
        _captureTokens(msg.sender, amount);

        // Pack the message to send to the other side of the linked token.
        CallMsg memory message = CallMsg({
            method: abi.encodePacked(bytes4(keccak256("receiveLinked(address,uint256)"))),
            params: abi.encode(recipient, amount)
        });
        IPCAddress memory destination = IPCAddress({
            subnetId: _linkedSubnet,
            rawAddress: FvmAddressHelper.from(_linkedContract)
        });

        // Route through GMP.
        committed = performIpcCall(destination, message, 0);

        // Record the unconfirmed transfer.
        _addUnconfirmedTransfer(committed.toHash(), msg.sender, amount);

        emit LinkedTokensSent({
            underlying: address(_underlying),
            sender: msg.sender,
            recipient: recipient,
            id: committed.toHash(),
            nonce: committed.nonce,
            value: amount
        });
    }

    // TODO make internal
    function lockAndTransferWithReturn(
        address receiver,
        uint256 amount
    ) external returns (IpcEnvelope memory envelope) {
        // Transfer and lock tokens on L1 using the inherited sendToken function
        return _linkedTransfer(receiver, amount);
    }

    // ----------------------------
    // Linked contract management.
    // ----------------------------

    // ----------------------------
    // IPC GMP entrypoints.
    // ----------------------------

    function _handleIpcCall(
        IpcEnvelope memory envelope,
        CallMsg memory callMsg
    ) internal override returns (bytes memory) {
        _validateInitialized();
        _validateEnvelope(envelope);
        _requireSelector(callMsg.method, "receiveLinked(address,uint256)");

        (address receiver, uint256 amount) = abi.decode(callMsg.params, (address, uint256));

        _receiveLinked(receiver, amount);
        return bytes("");
    }

    function _handleIpcResult(
        IpcEnvelope storage original,
        IpcEnvelope memory result,
        ResultMsg memory resultMsg
    ) internal override {
        _validateInitialized();
        _validateEnvelope(result);

        OutcomeType outcome = resultMsg.outcome;
        bool refund = outcome == OutcomeType.SystemErr || outcome == OutcomeType.ActorErr;

        _removeUnconfirmedTransfer({id: resultMsg.id, refund: refund});
    }

    function _receiveLinked(address recipient, uint256 amount) private {
        _validateTransfer(recipient, amount);

        _releaseTokens(recipient, amount);

        // Emit an event for the token unlock and transfer
        emit LinkedTokenReceived(recipient, amount);
    }

    // ----------------------------
    // Validation helpers.
    // ----------------------------

    function _validateInitialized() internal {
        require(_linkedContract != address(0), "linked token not initialized");
    }

    // Only accept messages from our linked token contract.
    // Made public for testing
    function _validateEnvelope(IpcEnvelope memory envelope) public {
        SubnetID memory subnetId = envelope.from.subnetId;
        if (!subnetId.equals(_linkedSubnet)) {
            revert InvalidOriginSubnet();
        }

        FvmAddress memory rawAddress = envelope.from.rawAddress;
        if (!rawAddress.equal(FvmAddressHelper.from(_linkedContract))) {
            revert InvalidOriginContract();
        }
    }

    function _requireSelector(bytes memory method, bytes memory signature) internal {
        if (method.length < 4) {
            revert InvalidEnvelope("short selector");
        }
        bytes4 coerced;
        assembly {
            coerced := mload(add(method, 32))
        }
        if (coerced != bytes4(keccak256(signature))) {
            revert InvalidEnvelope("invalid selector");
        }
    }

    function _validateTransfer(address receiver, uint256 amount) internal {
        if (receiver == address(0)) {
            revert TransferRejected(ERR_ZERO_ADDRESS);
        }
        if (amount == 0) {
            revert TransferRejected(ERR_AMOUNT_CANNOT_BE_ZERO);
        }
    }

    // ----------------------------
    // Unconfirmed transfers
    // ----------------------------

    function getUnconfirmedTransfer(bytes32 id) public view returns (address, uint256) {
        UnconfirmedTransfer storage details = _unconfirmedTransfers[id];
        return (details.sender, details.value);
    }

    // Method for the contract owner to manually drop an entry from unconfirmedTransfers
    function removeUnconfirmedTransfer(bytes32 id) external onlyOwner {
        _removeUnconfirmedTransfer(id, false);
    }

    function _addUnconfirmedTransfer(bytes32 hash, address sender, uint256 value) internal {
        _unconfirmedTransfers[hash] = UnconfirmedTransfer(sender, value);
    }

    function _removeUnconfirmedTransfer(bytes32 id, bool refund) internal {
        (address sender, uint256 value) = getUnconfirmedTransfer(id);
        delete _unconfirmedTransfers[id];

        if (refund) {
            require(sender != address(0), "internal error: no such unconfirmed transfer");
            _releaseTokens(sender, value);
        }
    }
}
