// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {ISashSegmentVerifier} from "./ISashSegmentVerifier.sol";

/// @title Sp1SashSegmentVerifier
/// @notice Production/testnet adapter verifying the BN254 PLONK proof of the M3 aggregator over the SASH segment
///         guests (`SPEC.md` §1), through the pinned SP1 v6.1.0 `SP1VerifierPlonk` (an immutable `ISP1Verifier`)
///         and the pinned `sashAggregatorVKey`.
/// @dev The outer wrap relies on BN254 pairing hardness, the Aztec Ignition universal SRS and SP1 v6 circuit
///      soundness (`SECURITY.md` §2 and §6).
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
        return SashTypes.VerifierKind.ZKVM_PLONK;
    }

    function verifyAggregatedSegments(bytes32 publicDigest, bytes calldata proofBytes) external view override {
        SP1_VERIFIER.verifyProof(SASH_AGGREGATOR_VKEY, abi.encodePacked(publicDigest), proofBytes);
    }
}
