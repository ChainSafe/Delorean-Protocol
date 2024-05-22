// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "elliptic-curve-solidity/contracts/EllipticCurve.sol";
import {IPCAddress} from "../../src/structs/Subnet.sol";
import {CallMsg, IpcMsgKind, IpcEnvelope} from "../../src/structs/CrossNet.sol";
import {IIpcHandler} from "../../sdk/interfaces/IIpcHandler.sol";
import {METHOD_SEND, EMPTY_BYTES} from "../../src/constants/Constants.sol";

library TestUtils {
    uint256 public constant GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798;
    uint256 public constant GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8;
    uint256 public constant AA = 0;
    uint256 public constant BB = 7;
    uint256 public constant PP = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F;

    function derivePubKey(uint256 privKey) external pure returns (uint256, uint256) {
        return EllipticCurve.ecMul(privKey, GX, GY, AA, PP);
    }

    function derivePubKeyBytes(uint256 privKey) public pure returns (bytes memory) {
        (uint256 pubKeyX, uint256 pubKeyY) = EllipticCurve.ecMul(privKey, GX, GY, AA, PP);
        return abi.encode(pubKeyX, pubKeyY);
    }

    function deriveValidatorPubKeyBytes(uint256 privKey) public pure returns (bytes memory) {
        (uint256 pubKeyX, uint256 pubKeyY) = EllipticCurve.ecMul(privKey, GX, GY, AA, PP);

        // https://github.com/ethereum/eth-keys/blob/master/README.md#keyapipublickeypublic_key_bytes

        return abi.encodePacked(uint8(0x4), pubKeyX, pubKeyY);
    }

    function getFourValidators(
        Vm vm
    ) internal returns (uint256[] memory validatorKeys, address[] memory addresses, uint256[] memory weights) {
        validatorKeys = new uint256[](4);
        validatorKeys[0] = 100;
        validatorKeys[1] = 200;
        validatorKeys[2] = 300;
        validatorKeys[3] = 400;

        addresses = new address[](4);
        addresses[0] = vm.addr(validatorKeys[0]);
        addresses[1] = vm.addr(validatorKeys[1]);
        addresses[2] = vm.addr(validatorKeys[2]);
        addresses[3] = vm.addr(validatorKeys[3]);

        weights = new uint256[](4);
        vm.deal(vm.addr(validatorKeys[0]), 1);
        vm.deal(vm.addr(validatorKeys[1]), 1);
        vm.deal(vm.addr(validatorKeys[2]), 1);
        vm.deal(vm.addr(validatorKeys[3]), 1);

        weights = new uint256[](4);
        weights[0] = 100;
        weights[1] = 100;
        weights[2] = 100;
        weights[3] = 100;
    }

    function getThreeValidators(
        Vm vm
    ) internal returns (uint256[] memory validatorKeys, address[] memory addresses, uint256[] memory weights) {
        validatorKeys = new uint256[](3);
        validatorKeys[0] = 100;
        validatorKeys[1] = 200;
        validatorKeys[2] = 300;

        addresses = new address[](3);
        addresses[0] = vm.addr(validatorKeys[0]);
        addresses[1] = vm.addr(validatorKeys[1]);
        addresses[2] = vm.addr(validatorKeys[2]);

        weights = new uint256[](3);
        vm.deal(vm.addr(validatorKeys[0]), 1);
        vm.deal(vm.addr(validatorKeys[1]), 1);
        vm.deal(vm.addr(validatorKeys[2]), 1);

        weights = new uint256[](3);
        weights[0] = 100;
        weights[1] = 101;
        weights[2] = 102;
    }

    function deriveValidatorAddress(uint8 seq) internal pure returns (address addr, bytes memory data) {
        data = new bytes(65);
        data[1] = bytes1(seq);

        // use data[1:] for the hash
        bytes memory dataSubset = new bytes(data.length - 1);
        for (uint i = 1; i < data.length; i++) {
            dataSubset[i - 1] = data[i];
        }

        addr = address(uint160(uint256(keccak256(dataSubset))));
    }

    function newValidator(
        uint256 key
    ) internal pure returns (address addr, uint256 privKey, bytes memory validatorKey) {
        privKey = key;
        bytes memory pubkey = derivePubKeyBytes(key);
        validatorKey = deriveValidatorPubKeyBytes(key);
        addr = address(uint160(uint256(keccak256(pubkey))));
    }

    function newValidators(
        uint256 n
    ) internal pure returns (address[] memory validators, uint256[] memory privKeys, bytes[] memory validatorKeys) {
        validatorKeys = new bytes[](n);
        validators = new address[](n);
        privKeys = new uint256[](n);

        for (uint i = 0; i < n; i++) {
            (address addr, uint256 key, bytes memory validatorKey) = newValidator(100 + i);
            validators[i] = addr;
            validatorKeys[i] = validatorKey;
            privKeys[i] = key;
        }

        return (validators, privKeys, validatorKeys);
    }

    function derivePubKey(uint8 seq) internal pure returns (address addr, bytes memory data) {
        data = new bytes(65);
        data[1] = bytes1(seq);

        // use data[1:] for the hash
        bytes memory dataSubset = new bytes(data.length - 1);
        for (uint i = 1; i < data.length; i++) {
            dataSubset[i - 1] = data[i];
        }

        addr = address(uint160(uint256(keccak256(dataSubset))));
    }

    function ensureBytesEqual(bytes memory _a, bytes memory _b) internal pure {
        require(_a.length == _b.length, "bytes len not equal");
        require(keccak256(_a) == keccak256(_b), "bytes not equal");
    }

    // Helper function to validate bytes4[] arrays
    function validateBytes4Array(
        bytes4[] memory array1,
        bytes4[] memory array2,
        string memory errorMessage
    ) internal pure {
        require(array1.length == array2.length, errorMessage);
        for (uint i = 0; i < array1.length; i++) {
            require(array1[i] == array2[i], errorMessage);
        }
    }

    function newXnetCallMsg(
        IPCAddress memory from,
        IPCAddress memory to,
        uint256 value,
        uint64 nonce
    ) internal pure returns (IpcEnvelope memory) {
        CallMsg memory message = CallMsg({method: abi.encodePacked(METHOD_SEND), params: EMPTY_BYTES});
        return
            IpcEnvelope({
                kind: IpcMsgKind.Call,
                from: from,
                to: to,
                value: value,
                message: abi.encode(message),
                nonce: nonce
            });
    }
}

contract MockIpcContract is IIpcHandler {
    /* solhint-disable-next-line unused-vars */
    function handleIpcMessage(IpcEnvelope calldata) external payable returns (bytes memory ret) {
        return EMPTY_BYTES;
    }
}

contract MockIpcContractFallback is IIpcHandler {
    /* solhint-disable-next-line unused-vars */
    function handleIpcMessage(IpcEnvelope calldata) external payable returns (bytes memory ret) {
        return EMPTY_BYTES;
    }

    fallback() external {
        revert();
    }
}

contract MockIpcContractRevert is IIpcHandler {
    bool public reverted = true;

    /* solhint-disable-next-line unused-vars */
    function handleIpcMessage(IpcEnvelope calldata) external payable returns (bytes memory) {
        // success execution of this methid will set reverted to false, by default it's true
        reverted = false;

        // since this reverts, `reverted` should always be true
        revert();
    }

    fallback() external {
        console.log("here2");
        revert();
    }
}

contract MockIpcContractPayable is IIpcHandler {
    /* solhint-disable-next-line unused-vars */
    function handleIpcMessage(IpcEnvelope calldata) external payable returns (bytes memory ret) {
        return EMPTY_BYTES;
    }

    receive() external payable {}
}
