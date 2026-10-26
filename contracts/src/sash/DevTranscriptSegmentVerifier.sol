// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {SashTypes} from "./SashTypes.sol";
import {ISashSegmentVerifier} from "./ISashSegmentVerifier.sol";

/// @title DevTranscriptSegmentVerifier
/// @notice Strictly local Foundry/Anvil (`chainid == 31337`) verifier for M0/M1 dev-mode testing.
/// @dev Reverts unconditionally on any chain other than `31337` and is rejected whenever
///      `VerifiableInferenceSettlement.allowDevVerifier == false`.
contract DevTranscriptSegmentVerifier is ISashSegmentVerifier {
    uint256 public constant DEV_CHAIN_ID = 31337;
    bytes32 public constant DEV_DOMAIN_TAG = keccak256("SASH_EVM_DEV_TRANSCRIPT_ONLY_V1");

    constructor() {
        if (block.chainid != DEV_CHAIN_ID) {
            revert SashTypes.DevChainOnly(block.chainid);
        }
    }

    function verifierKind() external pure override returns (SashTypes.VerifierKind) {
        return SashTypes.VerifierKind.DEV_TRANSCRIPT_ONLY;
    }

    function computeDevProof(bytes32 publicDigest) public pure returns (bytes memory) {
        return abi.encodePacked(sha256(abi.encodePacked(DEV_DOMAIN_TAG, publicDigest)));
    }

    function verifyAggregatedSegments(bytes32 publicDigest, bytes calldata proofBytes) external view override {
        if (block.chainid != DEV_CHAIN_ID) {
            revert SashTypes.DevChainOnly(block.chainid);
        }
        bytes32 expected = sha256(abi.encodePacked(DEV_DOMAIN_TAG, publicDigest));
        if (proofBytes.length != 32 || bytes32(proofBytes) != expected) {
            revert SashTypes.InvalidDevProofTag();
        }
    }
}
