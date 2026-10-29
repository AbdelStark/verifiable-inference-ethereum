// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";

/// @title SashGridVerifier
/// @notice Pure on-chain checks of SASH's 23-segment manifest roots, 22 residual-stream boundary links,
///         prompt prefix, autoregressive loop closure, answer and EOS termination, plus the sha256
///         `publicDigest` that binds the segment claims and both token vectors to the proof (`SPEC.md` §1).
library SashGridVerifier {
    /// @notice `sha256` over token ids, each encoded as a 32-byte big-endian word.
    function computeTokenHash(uint32[] calldata ids) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(ids));
    }

    /// @notice `sha256(preRoot[0] || modelRoot[0] || fixedHash[0] || ... || fixedHash[n-1])`.
    function computeManifestRootsHash(SashTypes.SegmentClaim[] calldata segments) internal pure returns (bytes32) {
        uint256 n = segments.length;
        bytes memory buf = new bytes(96 * n);
        for (uint256 i = 0; i < n; ++i) {
            bytes32 preRt = segments[i].preRoot;
            bytes32 modelRt = segments[i].modelRoot;
            bytes32 fixedH = segments[i].fixedHash;
            uint256 baseOff = 96 * i;
            assembly ("memory-safe") {
                mstore(add(add(buf, 32), baseOff), preRt)
                mstore(add(add(buf, 64), baseOff), modelRt)
                mstore(add(add(buf, 96), baseOff), fixedH)
            }
        }
        return sha256(buf);
    }

    function computePublicDigest(
        bytes32 modelId,
        SashTypes.SegmentClaim[] calldata segments,
        uint32[] calldata tokIn,
        uint32[] calldata tokOut
    ) internal pure returns (bytes32) {
        uint256 n = segments.length;
        // 32 B modelId + 4 B BE len + n * 162 B per SegmentClaim + 2 * 32 B token hashes
        bytes memory buf = new bytes(100 + 162 * n);
        assembly ("memory-safe") {
            mstore(add(buf, 32), modelId)
        }
        // casting to 'uint32' is safe because a calldata array cannot hold 2^32 162-byte claims
        // forge-lint: disable-next-line(unsafe-typecast)
        writeU32BE(buf, 32, uint32(n));

        for (uint256 i = 0; i < n; ++i) {
            SashTypes.SegmentClaim calldata s = segments[i];
            uint256 o = 36 + 162 * i;
            writeU16BE(buf, o, s.segmentIndex);

            bytes32 preRt = s.preRoot;
            bytes32 modelRt = s.modelRoot;
            bytes32 resIn = s.residualInRoot;
            bytes32 resOut = s.residualOutRoot;
            bytes32 fixedH = s.fixedHash;
            assembly ("memory-safe") {
                let ptr := add(add(buf, 32), add(o, 2))
                mstore(ptr, preRt)
                mstore(add(ptr, 32), modelRt)
                mstore(add(ptr, 64), resIn)
                mstore(add(ptr, 96), resOut)
                mstore(add(ptr, 128), fixedH)
            }
        }

        bytes32 tokInHash = computeTokenHash(tokIn);
        bytes32 tokOutHash = computeTokenHash(tokOut);
        uint256 tail = 36 + 162 * n;
        assembly ("memory-safe") {
            let ptr := add(add(buf, 32), tail)
            mstore(ptr, tokInHash)
            mstore(add(ptr, 32), tokOutHash)
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
            // one mismatch rejects the whole run, so the loop fails fast
            // forge-lint: disable-next-line(require-revert-in-loop)
            if (tokIn[j] != promptIds[j]) revert SashTypes.PromptTokenMismatch(j);
        }
        for (uint256 j = 0; j < aLen - 1; ++j) {
            if (tokIn[pLen + j] != tokOut[pLen - 1 + j]) {
                // one mismatch rejects the whole run, so the loop fails fast
                // forge-lint: disable-next-line(require-revert-in-loop)
                revert SashTypes.AutoregressiveLoopMismatch(j);
            }
        }
        for (uint256 j = 0; j < aLen; ++j) {
            // one mismatch rejects the whole run, so the loop fails fast
            // forge-lint: disable-next-line(require-revert-in-loop)
            if (tokOut[pLen - 1 + j] != answerIds[j]) revert SashTypes.AnswerTokenMismatch(j);
        }
        uint32 finalTok = tokOut[pLen + aLen - 2];
        if (finalTok != manifest.eosTokenId) {
            revert SashTypes.MissingEosTermination(finalTok, manifest.eosTokenId);
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
