// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";

/// @title ISP1Verifier
/// @notice Minimal interface matching Succinct's deployed `SP1VerifierGateway` on Ethereum / Sepolia.
interface ISP1Verifier {
    function verifyProof(bytes32 programVKey, bytes calldata publicValues, bytes calldata proofBytes) external view;
}

/// @title ISashSegmentVerifier
/// @notice Pluggable segment proof verifier interface isolating proof-system adapters from SASH grid logic.
interface ISashSegmentVerifier {
    function verifierKind() external pure returns (SashTypes.VerifierKind);

    function verifyAggregatedSegments(bytes32 publicDigest, bytes calldata proofBytes) external view;
}
