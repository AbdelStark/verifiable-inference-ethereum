// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";
import {ISashSegmentVerifier} from "./ISashSegmentVerifier.sol";
import {SashModelRegistry} from "./SashModelRegistry.sol";
import {SashGridVerifier} from "./SashGridVerifier.sol";

/// @title VerifiableInferenceSettlement
/// @notice Top-level EVM settlement contract verifying quantized SASH LLM inference runs (`sash-evm-execution-integrity/v1`).
/// @dev Establishes execution integrity for one run of an enrolled quantized model manifest.
///      Does not establish floating-point equivalence, zero-knowledge hiding, or output safety.
contract VerifiableInferenceSettlement {
    SashModelRegistry public immutable REGISTRY;
    ISashSegmentVerifier public immutable SEGMENT_VERIFIER;
    bool public immutable ALLOW_DEV_VERIFIER;

    mapping(bytes32 => bool) public settledRuns;

    event InferenceVerified(
        bytes32 indexed runId,
        bytes32 indexed modelId,
        bytes32 indexed publicDigest,
        bytes32 promptHash,
        bytes32 answerHash,
        SashTypes.VerifierKind verifierKind
    );

    constructor(address registry, address segmentVerifier, bool allowDevVerifier) {
        REGISTRY = SashModelRegistry(registry);
        SEGMENT_VERIFIER = ISashSegmentVerifier(segmentVerifier);
        ALLOW_DEV_VERIFIER = allowDevVerifier;

        if (!allowDevVerifier && SEGMENT_VERIFIER.verifierKind() == SashTypes.VerifierKind.DEV_TRANSCRIPT_ONLY) {
            revert SashTypes.DevVerifierForbidden();
        }
    }

    function verifyAndSettle(
        bytes32 modelId,
        SashTypes.SegmentClaim[] calldata segments,
        uint32[] calldata tokIn,
        uint32[] calldata tokOut,
        uint32[] calldata promptIds,
        uint32[] calldata answerIds,
        bytes calldata proofBytes
    ) external returns (bytes32 runId) {
        SashTypes.VerifierKind kind = SEGMENT_VERIFIER.verifierKind();
        if (!ALLOW_DEV_VERIFIER && kind == SashTypes.VerifierKind.DEV_TRANSCRIPT_ONLY) {
            revert SashTypes.DevVerifierForbidden();
        }

        SashTypes.ModelManifest memory manifest = REGISTRY.getManifest(modelId);

        bytes32 publicDigest = SashGridVerifier.computePublicDigest(modelId, segments, tokIn, tokOut);
        bytes32 promptHash = SashGridVerifier.computeTokenHash(promptIds);
        bytes32 answerHash = SashGridVerifier.computeTokenHash(answerIds);
        runId = sha256(abi.encodePacked(modelId, publicDigest, promptHash, answerHash));

        if (settledRuns[runId]) {
            revert SashTypes.RunAlreadySettled(runId);
        }

        // 1. Verify succinct cryptographic proof (or chain-gated dev tag on chainid 31337) over `publicDigest`,
        //    which binds the segment claims and the `tokIn` / `tokOut` vectors the proof's token checks used.
        SEGMENT_VERIFIER.verifyAggregatedSegments(publicDigest, proofBytes);

        // 2. Verify SASH 23-segment manifest roots, 22 residual links, prompt prefix, autoregressive loop closure,
        //    answer and EOS termination natively on EVM.
        SashGridVerifier.verifyGrid(manifest, segments, tokIn, tokOut, promptIds, answerIds);

        settledRuns[runId] = true;
        // the external calls before this event are view calls (STATICCALL) and cannot reenter
        // forge-lint: disable-next-line(reentrancy-events)
        emit InferenceVerified(runId, modelId, publicDigest, promptHash, answerHash, kind);
    }
}
