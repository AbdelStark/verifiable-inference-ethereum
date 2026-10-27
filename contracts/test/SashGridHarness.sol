// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {SashGridVerifier} from "../src/sash/SashGridVerifier.sol";

/// External `pure` entry points to the internal `SashGridVerifier` library, so a gas snapshot of the
/// call frame measures the library alone. `pure` calls aren't isolated, so no intrinsic gas is counted.
contract SashGridHarness {
    function verifyGrid(
        SashTypes.ModelManifest calldata manifest,
        SashTypes.SegmentClaim[] calldata segments,
        uint32[] calldata tokIn,
        uint32[] calldata tokOut,
        uint32[] calldata promptIds,
        uint32[] calldata answerIds
    ) external pure {
        SashGridVerifier.verifyGrid(manifest, segments, tokIn, tokOut, promptIds, answerIds);
    }

    function computePublicDigest(
        bytes32 modelId,
        SashTypes.SegmentClaim[] calldata segments,
        uint32[] calldata tokIn,
        uint32[] calldata tokOut
    ) external pure returns (bytes32) {
        return SashGridVerifier.computePublicDigest(modelId, segments, tokIn, tokOut);
    }
}
