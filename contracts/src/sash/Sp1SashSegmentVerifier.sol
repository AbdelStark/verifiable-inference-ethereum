// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {ISashSegmentVerifier} from "./ISashSegmentVerifier.sol";

/// @title Sp1SashSegmentVerifier
/// @notice Production/testnet adapter verifying the BN254 Groth16/Plonk proof of the M3 aggregator over the
///         SASH segment guests (`SPEC.md` §1) via an immutable `ISP1Verifier` and pinned `sashAggregatorVKey`.
/// @dev Outer wrapper relies on BN254 pairing hardness, SRS ceremony integrity, and SP1 circuit soundness.
contract Sp1SashSegmentVerifier is ISashSegmentVerifier {
    ISP1Verifier public immutable SP1_VERIFIER;
    bytes32 public immutable SASH_AGGREGATOR_VKEY;

    error ZeroVerifierAddress();
    error ZeroProgramVKey();

    constructor(address sp1Verifier, bytes32 sashAggregatorVKey) {
        if (sp1Verifier == address(0)) revert ZeroVerifierAddress();
        if (sashAggregatorVKey == bytes32(0)) revert ZeroProgramVKey();
        SP1_VERIFIER = ISP1Verifier(sp1Verifier);
        SASH_AGGREGATOR_VKEY = sashAggregatorVKey;
    }

    function verifierKind() external pure override returns (SashTypes.VerifierKind) {
        return SashTypes.VerifierKind.ZKVM_GROTH16;
    }

    function verifyAggregatedSegments(bytes32 publicDigest, bytes calldata proofBytes) external view override {
        SP1_VERIFIER.verifyProof(SASH_AGGREGATOR_VKEY, abi.encodePacked(publicDigest), proofBytes);
    }
}
