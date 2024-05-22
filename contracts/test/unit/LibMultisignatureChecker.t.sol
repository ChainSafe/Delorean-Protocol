// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import {TestUtils} from "../helpers/TestUtils.sol";
import {MultisignatureChecker} from "../../src/lib/LibMultisignatureChecker.sol";
import {ECDSA} from "openzeppelin-contracts/utils/cryptography/ECDSA.sol";
import "elliptic-curve-solidity/contracts/EllipticCurve.sol";

contract MultisignatureCheckerTest is StdInvariant, Test {
    /// @dev `derivePubKey` is going to be used only in tests. This test is not complete, and covers only usage of
    /// foundry tools.
    function testPublicKeyDerivation(uint256 key) public pure {
        vm.assume(key > 2);
        vm.assume(key < 10000000000000000);

        (uint256 pubKeyX, uint256 pubKeyY) = TestUtils.derivePubKey(key);
        address signer = address(uint160(uint256(keccak256(abi.encode(pubKeyX, pubKeyY)))));

        bytes32 hash = keccak256(abi.encodePacked("test"));

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, hash);
        bytes memory signature = abi.encodePacked(r, s, v);

        address s1 = ECDSA.recover(hash, signature);
        require(s1 == signer, "s1 == signer");
    }

    function testBasicSignerInterface(uint256 key) public pure {
        vm.assume(key > 2);
        vm.assume(key < 10000000000000000);
        address signer = vm.addr(key);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, hash);
        bytes memory signature = abi.encodePacked(r, s, v);

        address s1 = ECDSA.recover(hash, signature);
        require(s1 == signer, "s1 == signer");
    }

    function testMultiSignatureChecker_Weighted_OneSignature(uint256 key) public pure {
        vm.assume(key > 2);
        vm.assume(key < 10000000000000000);
        address signer = vm.addr(key);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, hash);
        bytes memory signatureBytes = abi.encodePacked(r, s, v);

        require(signatureBytes.length == 65, "signatureBytes.length == 65");

        address[] memory signers = new address[](1);
        signers[0] = signer;

        uint256[] memory weights = new uint256[](1);
        weights[0] = 10;

        bytes[] memory signatures = new bytes[](1);
        signatures[0] = signatureBytes;

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            10,
            hash,
            signatures
        );
        require(valid == true, "valid == true");
        require(err == MultisignatureChecker.Error.Nil, "err == Nil");
    }

    function testMultiSignatureChecker_Weighted_FourSignatures() public pure {
        uint256 PRIVATE_KEY_BASE = 1000;
        address[] memory signers = new address[](4);
        uint256[] memory weights = new uint256[](4);
        bytes[] memory signatures = new bytes[](4);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        for (uint256 i = 0; i < 4; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(PRIVATE_KEY_BASE + i, hash);
            signatures[i] = abi.encodePacked(r, s, v);
            signers[i] = vm.addr(PRIVATE_KEY_BASE + i);
            weights[i] = 10;
        }

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            30,
            hash,
            signatures
        );
        require(valid == true, "valid == true");
        require(err == MultisignatureChecker.Error.Nil, "err == Nil");
    }

    function testMultiSignatureChecker_FourSignatures_WeightsFuzzed(
        uint256 w1,
        uint256 w2,
        uint256 w3,
        uint256 w4,
        uint256 threshold
    ) public pure {
        vm.assume(w1 < threshold / 4);
        vm.assume(w2 < threshold / 4);
        vm.assume(w3 < threshold / 4);
        vm.assume(w4 < threshold / 4);

        uint256 PRIVATE_KEY_BASE = 1000;
        address[] memory signers = new address[](4);
        uint256[] memory weights = new uint256[](4);
        bytes[] memory signatures = new bytes[](4);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        for (uint256 i = 0; i < 4; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(PRIVATE_KEY_BASE + i, hash);
            signatures[i] = abi.encodePacked(r, s, v);
            signers[i] = vm.addr(PRIVATE_KEY_BASE + i);
        }
        weights[0] = w1;
        weights[1] = w2;
        weights[2] = w3;
        weights[3] = w4;

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            threshold,
            hash,
            signatures
        );
        require(
            (valid == true && err == MultisignatureChecker.Error.Nil) ||
                (valid == false && err == MultisignatureChecker.Error.WeightsSumLessThanThreshold),
            "Error.Nil or WeightsSumLessThanThreshold"
        );
    }

    function testMultiSignatureChecker_Weighted_FourSignatures_Fuzz(
        uint256 k1,
        uint256 k2,
        uint256 k3,
        uint256 k4
    ) public pure {
        address[] memory signers = new address[](4);
        uint256[] memory weights = new uint256[](4);
        uint256[] memory keys = new uint256[](4);
        bytes[] memory signatures = new bytes[](4);

        vm.assume(k1 > 2);
        vm.assume(k1 < 10000000000000000);
        vm.assume(k2 > 2);
        vm.assume(k2 < 10000000000000000);
        vm.assume(k3 > 2);
        vm.assume(k3 < 10000000000000000);
        vm.assume(k4 > 2);
        vm.assume(k4 < 10000000000000000);

        keys[0] = k1;
        keys[1] = k2;
        keys[2] = k3;
        keys[3] = k4;

        bytes32 hash = keccak256(abi.encodePacked("test"));

        for (uint256 i = 0; i < 4; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(keys[i], hash);
            signatures[i] = abi.encodePacked(r, s, v);
            signers[i] = vm.addr(keys[i]);
            weights[i] = 10;
        }

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            30,
            hash,
            signatures
        );
        require(valid == true, "valid == true");
        require(err == MultisignatureChecker.Error.Nil, "err == Nil");
    }

    function testMultiSignatureChecker_Weighted_InvalidSignaturesLength() public pure {
        bytes32 hash = keccak256(abi.encodePacked("test"));

        address[] memory signers = new address[](1);
        signers[0] = vm.addr(101);

        uint256[] memory weights = new uint256[](1);
        weights[0] = 10;

        // signatures is empty
        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            10,
            hash,
            new bytes[](0)
        );
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.EmptySignatures, "for empty signatures");

        // signature has one signature with incorrect length
        bytes[] memory signatures = new bytes[](1);
        signatures[0] = abi.encodePacked(hash);
        (valid, err) = MultisignatureChecker.isValidWeightedMultiSignature(signers, weights, 10, hash, signatures);
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.InvalidSignature, "signature length is 32");

        signatures[0] = bytes.concat(abi.encodePacked(hash), abi.encodePacked(hash), abi.encodePacked(hash));
        (valid, err) = MultisignatureChecker.isValidWeightedMultiSignature(signers, weights, 10, hash, signatures);
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.InvalidSignature, "signature length is 96");

        signatures = new bytes[](2);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(100, hash);
        signatures[0] = abi.encodePacked(r, s, v);
        signatures[1] = abi.encodePacked(r, s, v);
        (valid, err) = MultisignatureChecker.isValidWeightedMultiSignature(signers, weights, 10, hash, signatures);
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.InvalidArrayLength, "different array lengths");
    }

    function testMultiSignatureChecker_Weighted_InvalidSignatureInMultisig() public pure {
        uint256 PRIVATE_KEY_BASE = 1000;
        address[] memory signers = new address[](4);
        bytes[] memory signatures = new bytes[](4);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        bytes32 b;

        uint256[] memory weights = new uint256[](4);

        for (uint256 i = 0; i < 4; i++) {
            (uint8 v, bytes32 r, ) = vm.sign(PRIVATE_KEY_BASE + i, hash);
            signatures[i] = abi.encodePacked(r, b, v);
            signers[i] = vm.addr(PRIVATE_KEY_BASE + i);
            weights[i] = 10;
        }

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            30,
            hash,
            signatures
        );
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.InvalidSignature, "err == InvalidSignature");
    }

    function testMultiSignatureChecker_Weighted_InvalidSignatureOfSigner() public pure {
        uint256 PRIVATE_KEY_BASE = 1000;
        address[] memory signers = new address[](2);
        uint256[] memory weights = new uint256[](2);
        bytes[] memory signatures = new bytes[](2);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        for (uint256 i = 0; i < 2; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(PRIVATE_KEY_BASE + i, hash);
            signatures[i] = abi.encodePacked(r, s, v);
            weights[i] = 10;
        }

        signers[0] = vm.addr(PRIVATE_KEY_BASE + 1);
        signers[1] = vm.addr(PRIVATE_KEY_BASE);

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            10,
            hash,
            signatures
        );
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.InvalidSignatory, "err == InvalidSigner");
    }

    function testMultiSignatureChecker_Weighted_LessThanThreshold() public pure {
        uint256 PRIVATE_KEY_BASE = 1000;
        address[] memory signers = new address[](2);
        uint256[] memory weights = new uint256[](2);
        bytes[] memory signatures = new bytes[](2);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        for (uint256 i = 0; i < 2; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(PRIVATE_KEY_BASE + i, hash);
            signatures[i] = abi.encodePacked(r, s, v);
            weights[i] = 10;
            signers[i] = vm.addr(PRIVATE_KEY_BASE + i);
        }

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            100,
            hash,
            signatures
        );
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.WeightsSumLessThanThreshold, "err == WeightsSumLessThanThreshold");
    }

    function testMultiSignatureChecker_Weighted_InvalidNumberOfWeights() public pure {
        uint256 PRIVATE_KEY_BASE = 1000;
        address[] memory signers = new address[](2);
        uint256[] memory weights = new uint256[](1);
        bytes[] memory signatures = new bytes[](2);

        bytes32 hash = keccak256(abi.encodePacked("test"));

        for (uint256 i = 0; i < 2; i++) {
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(PRIVATE_KEY_BASE + i, hash);
            signatures[i] = abi.encodePacked(r, s, v);
        }
        weights[0] = 1;

        signers[0] = vm.addr(PRIVATE_KEY_BASE + 1);
        signers[1] = vm.addr(PRIVATE_KEY_BASE);

        (bool valid, MultisignatureChecker.Error err) = MultisignatureChecker.isValidWeightedMultiSignature(
            signers,
            weights,
            10,
            hash,
            signatures
        );
        require(valid == false, "valid == false");
        require(err == MultisignatureChecker.Error.InvalidArrayLength, "err == InvalidArrayLength");
    }
}
