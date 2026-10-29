// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

/// @title SashTypes
/// @notice Canonical data structures, verifier modes, and typed errors for SASH EVM settlement.
library SashTypes {
    enum VerifierKind {
        DEV_TRANSCRIPT_ONLY,
        ZKVM_GROTH16
    }

    struct ModelManifest {
        uint16 numSegments;
        uint16 seqLen;
        uint32 eosTokenId;
        bytes32 manifestRootsHash;
        bool active;
    }

    struct SegmentClaim {
        uint16 segmentIndex;
        bytes32 preRoot;
        bytes32 modelRoot;
        bytes32 residualInRoot;
        bytes32 residualOutRoot;
        bytes32 fixedHash;
    }

    error ZeroOwner();
    error UnauthorizedOwner(address caller);
    error ModelAlreadyEnrolled(bytes32 modelId);
    error ModelNotActive(bytes32 modelId);
    error InvalidManifestParameters();
    error InvalidSegmentCount(uint256 got, uint256 expected);
    error InvalidSegmentIndex(uint256 position, uint16 got);
    error ManifestRootMismatch(bytes32 got, bytes32 expected);
    error ResidualBoundaryMismatch(uint256 linkIndex);
    error HeadMustNotCarryOutRoot();
    error InvalidSequenceLength();
    error InvalidTokenWindow();
    error PromptTokenMismatch(uint256 index);
    error AutoregressiveLoopMismatch(uint256 step);
    error AnswerTokenMismatch(uint256 index);
    error MissingEosTermination(uint32 got, uint32 expected);
    error DevChainOnly(uint256 chainId);
    error DevVerifierForbidden();
    error InvalidDevProofTag();
    error RunAlreadySettled(bytes32 runId);
}
