// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.23;

import {ECDSA} from "openzeppelin-contracts/utils/cryptography/ECDSA.sol";

/// @title Multi-signature ECDSA verification helper.
library MultisignatureChecker {
    uint8 private constant SIGNATURE_LENGTH = 65;

    enum Error {
        Nil,
        InvalidArrayLength,
        EmptySignatures,
        InvalidSignatory,
        InvalidSignature,
        WeightsSumLessThanThreshold
    }

    /**
     * @notice Checks if a weighted multi-signature is valid for a given message hash, set of signatories, set of weights, and set of signatures.
     * @dev Signatures are validated using `ECDSA.recover`.
     *      The multi-signature fails if the sum of the signatory weights is less than the threshold.
     *      Signatories in `signatories` and  signatures in `signatures` must have the same order.
     * @param signatories The addresses of the signatories.
     * @param weights The weights of the signatories.
     * @param threshold The number that must be reach to consider `signatures` valid.
     * @param hash of the verified data.
     * @param signatures Packed signatures. Each signature is in `({bytes32 r}{bytes32 s}{uint8 v})` format.
     */
    function isValidWeightedMultiSignature(
        address[] memory signatories,
        uint256[] memory weights,
        uint256 threshold,
        bytes32 hash,
        bytes[] memory signatures
    ) internal pure returns (bool, Error) {
        bool valid = true;
        uint256 weight;

        uint256 signaturesNumber = signatures.length;
        if (signaturesNumber == 0) {
            return (!valid, Error.EmptySignatures);
        }

        if (signaturesNumber != signatories.length || signaturesNumber != weights.length) {
            return (!valid, Error.InvalidArrayLength);
        }

        for (uint256 i; i < signaturesNumber; ) {
            (address recovered, ECDSA.RecoverError ecdsaErr, ) = ECDSA.tryRecover({
                hash: hash,
                signature: signatures[i]
            });
            if (ecdsaErr != ECDSA.RecoverError.NoError) {
                return (!valid, Error.InvalidSignature);
            }
            if (recovered != signatories[i]) {
                return (!valid, Error.InvalidSignatory);
            }
            weight = weight + weights[i];
            unchecked {
                ++i;
            }
        }
        if (weight >= threshold) {
            return (valid, Error.Nil);
        }
        return (!valid, Error.WeightsSumLessThanThreshold);
    }
}
