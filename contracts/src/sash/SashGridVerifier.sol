// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "../field/M31.sol";
import {QM31} from "../field/QM31.sol";
import {Blake2sM31Channel} from "../crypto/Blake2sM31Channel.sol";
import {SashTypes} from "./SashTypes.sol";

/// @title SashGridVerifier
/// @notice Pure on-chain verifier enforcing SASH's 23-segment manifest roots, 22 residual-stream Merkle
///         boundary links, `Blake2sM31Channel` Fiat-Shamir `w_residual_stream` derivation, `QM31` Horner
///         token commitments, prompt prefix equality, autoregressive loop closure, and EOS termination.
library SashGridVerifier {
    function computeManifestRootsHash(SashTypes.SegmentClaim[] calldata segments) internal pure returns (bytes32) {
        uint256 n = segments.length;
        bytes memory buf = new bytes(64 * n);
        for (uint256 i = 0; i < n; ++i) {
            bytes32 preRt = segments[i].preRoot;
            bytes32 modelRt = segments[i].modelRoot;
            uint256 baseOff = 64 * i;
            assembly ("memory-safe") {
                mstore(add(add(buf, 32), baseOff), preRt)
                mstore(add(add(buf, 64), baseOff), modelRt)
            }
        }
        return sha256(buf);
    }

    function computePublicDigest(bytes32 modelId, SashTypes.SegmentClaim[] calldata segments)
        internal
        pure
        returns (bytes32)
    {
        uint256 n = segments.length;
        // 32 B modelId + 4 B BE len + n * 180 B per SegmentClaim
        bytes memory buf = new bytes(36 + 180 * n);
        assembly ("memory-safe") {
            mstore(add(buf, 32), modelId)
        }
        // casting to 'uint32' is safe because a calldata array cannot hold 2^32 180-byte claims
        // forge-lint: disable-next-line(unsafe-typecast)
        writeU32BE(buf, 32, uint32(n));

        for (uint256 i = 0; i < n; ++i) {
            SashTypes.SegmentClaim calldata s = segments[i];
            uint256 o = 36 + 180 * i;
            writeU16BE(buf, o, s.segmentIndex);
            writeU16BE(buf, o + 2, s.relationDraws);

            bytes32 preRt = s.preRoot;
            bytes32 modelRt = s.modelRoot;
            bytes32 traceRt = s.traceRoot;
            bytes32 resIn = s.residualInRoot;
            bytes32 resOut = s.residualOutRoot;
            assembly ("memory-safe") {
                let ptr := add(add(buf, 32), add(o, 4))
                mstore(ptr, preRt)
                mstore(add(ptr, 32), modelRt)
                mstore(add(ptr, 64), traceRt)
                mstore(add(ptr, 96), resIn)
                mstore(add(ptr, 128), resOut)
            }
            writeU32BE(buf, o + 164, s.tokenHornerClaim.a0);
            writeU32BE(buf, o + 168, s.tokenHornerClaim.a1);
            writeU32BE(buf, o + 172, s.tokenHornerClaim.b0);
            writeU32BE(buf, o + 176, s.tokenHornerClaim.b1);
        }
        return sha256(buf);
    }

    function verifyGrid(
        SashTypes.ModelManifest memory manifest,
        SashTypes.SegmentClaim[] calldata segments,
        uint32[] calldata tokIn,
        uint32[] calldata tokOut,
        uint32[] calldata promptIds,
        uint32[] calldata answerIds
    ) internal pure {
        uint256 expectedN = manifest.numSegments;
        if (expectedN < 2 || segments.length != expectedN) {
            revert SashTypes.InvalidSegmentCount(segments.length, expectedN);
        }

        for (uint256 pos = 0; pos < expectedN; ++pos) {
            if (segments[pos].segmentIndex != pos) {
                // one mismatch rejects the whole run, so the loop fails fast
                // forge-lint: disable-next-line(require-revert-in-loop)
                revert SashTypes.InvalidSegmentIndex(pos, segments[pos].segmentIndex);
            }
            QM31.checkCanonical(segments[pos].tokenHornerClaim);
        }

        bytes32 rootsHash = computeManifestRootsHash(segments);
        if (rootsHash != manifest.manifestRootsHash) {
            revert SashTypes.ManifestRootMismatch(rootsHash, manifest.manifestRootsHash);
        }

        uint256 last = expectedN - 1;
        for (uint256 i = 0; i < last; ++i) {
            if (segments[i].residualOutRoot != segments[i + 1].residualInRoot) {
                // one mismatch rejects the whole run, so the loop fails fast
                // forge-lint: disable-next-line(require-revert-in-loop)
                revert SashTypes.ResidualBoundaryMismatch(i);
            }
        }
        if (segments[last].residualOutRoot != bytes32(0)) {
            revert SashTypes.HeadMustNotCarryOutRoot();
        }

        if (segments[0].relationDraws != manifest.b0RelDraws) {
            revert SashTypes.RelationDrawCountMismatch(0, segments[0].relationDraws, manifest.b0RelDraws);
        }
        if (segments[last].relationDraws != manifest.headRelDraws) {
            revert SashTypes.RelationDrawCountMismatch(last, segments[last].relationDraws, manifest.headRelDraws);
        }

        uint256 s = manifest.seqLen;
        if (tokIn.length != s || tokOut.length != s) {
            revert SashTypes.InvalidSequenceLength();
        }
        uint256 pLen = promptIds.length;
        uint256 aLen = answerIds.length;
        if (pLen == 0 || aLen == 0 || (pLen + aLen - 1) != s) {
            revert SashTypes.InvalidTokenWindow();
        }

        for (uint256 j = 0; j < pLen; ++j) {
            uint32 pt = promptIds[j];
            M31.checkCanonical(pt);
            // one mismatch rejects the whole run, so the loop fails fast
            // forge-lint: disable-next-line(require-revert-in-loop)
            if (tokIn[j] != pt) revert SashTypes.PromptTokenMismatch(j);
        }
        for (uint256 j = 0; j < aLen - 1; ++j) {
            if (tokIn[pLen + j] != tokOut[pLen - 1 + j]) {
                // one mismatch rejects the whole run, so the loop fails fast
                // forge-lint: disable-next-line(require-revert-in-loop)
                revert SashTypes.AutoregressiveLoopMismatch(j);
            }
        }
        for (uint256 j = 0; j < aLen; ++j) {
            uint32 at = answerIds[j];
            M31.checkCanonical(at);
            // one mismatch rejects the whole run, so the loop fails fast
            // forge-lint: disable-next-line(require-revert-in-loop)
            if (tokOut[pLen - 1 + j] != at) revert SashTypes.AnswerTokenMismatch(j);
        }
        uint32 finalTok = tokOut[pLen + aLen - 2];
        if (finalTok != manifest.eosTokenId) {
            revert SashTypes.MissingEosTermination(finalTok, manifest.eosTokenId);
        }

        // Replay Fiat-Shamir channel on segment 0 (`B0L0`) and check ascending Horner(tokIn).
        QM31.Elem memory wB0 = Blake2sM31Channel.deriveWResidualStream(
            segments[0].preRoot, segments[0].modelRoot, segments[0].traceRoot, segments[0].relationDraws
        );
        QM31.Elem memory embedHorner = QM31.hornerAsc(tokIn, wB0);
        if (!QM31.eq(embedHorner, segments[0].tokenHornerClaim)) {
            revert SashTypes.EmbedTokenHornerMismatch();
        }

        // Replay Fiat-Shamir channel on final segment (`head`) and check descending Horner(tokOut).
        QM31.Elem memory wHead = Blake2sM31Channel.deriveWResidualStream(
            segments[last].preRoot, segments[last].modelRoot, segments[last].traceRoot, segments[last].relationDraws
        );
        QM31.Elem memory headHorner = QM31.hornerDesc(tokOut, wHead);
        if (!QM31.eq(headHorner, segments[last].tokenHornerClaim)) {
            revert SashTypes.HeadTokenHornerMismatch();
        }
    }

    function writeU16BE(bytes memory buf, uint256 offset, uint16 v) private pure {
        bytes2 b = bytes2(v);
        buf[offset] = b[0];
        buf[offset + 1] = b[1];
    }

    function writeU32BE(bytes memory buf, uint256 offset, uint32 v) private pure {
        bytes4 b = bytes4(v);
        buf[offset] = b[0];
        buf[offset + 1] = b[1];
        buf[offset + 2] = b[2];
        buf[offset + 3] = b[3];
    }
}
