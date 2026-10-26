// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";

/// @title ISashSegmentVerifier
/// @notice Pluggable segment proof verifier interface isolating proof-system adapters from SASH grid logic.
interface ISashSegmentVerifier {
    function verifierKind() external pure returns (SashTypes.VerifierKind);

    function verifyAggregatedSegments(bytes32 publicDigest, bytes calldata proofBytes) external view;
}
