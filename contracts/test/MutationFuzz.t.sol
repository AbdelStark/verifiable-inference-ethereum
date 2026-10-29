// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// Single-field mutation fuzz control (`SPEC.md` §2.1, row F).
contract MutationFuzzTest is TinyLlamaBase {
    /// Flips bits in one field of the valid run and submits it with the original dev proof
    /// (`SPEC.md` §2.1, row F). The proof is never re-derived for the mutated digest: nothing but the
    /// digest binds `segments[0].residualInRoot`, so that mutation would settle.
    function testFuzzSingleFieldMutationReverts(
        uint8 target,
        uint256 index,
        bytes32 mask,
        uint32 mask32,
        uint16 mask16,
        uint8 mask8
    ) public {
        bytes32 modelId = TinyLlamaFixture.MODEL_ID;
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        uint32[] memory ti = TinyLlamaFixture.tokIn();
        uint32[] memory to = TinyLlamaFixture.tokOut();
        uint32[] memory pr = TinyLlamaFixture.promptIds();
        uint32[] memory ans = TinyLlamaFixture.answerIds();
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);

        target %= 7;
        if (target == 0) {
            vm.assume(mask != 0);
            modelId ^= mask;
        } else if (target == 1) {
            SashTypes.SegmentClaim memory seg = segs[index % segs.length];
            uint256 field = (index / segs.length) % 6;
            if (field == 0) {
                vm.assume(mask16 != 0);
                seg.segmentIndex ^= mask16;
            } else {
                vm.assume(mask != 0);
                if (field == 1) seg.preRoot ^= mask;
                else if (field == 2) seg.modelRoot ^= mask;
                else if (field == 3) seg.residualInRoot ^= mask;
                else if (field == 4) seg.residualOutRoot ^= mask;
                else seg.fixedHash ^= mask;
            }
        } else if (target == 6) {
            vm.assume(mask8 != 0);
            proof[index % proof.length] ^= bytes1(mask8);
        } else {
            vm.assume(mask32 != 0);
            uint32[] memory ids = target == 2 ? ti : target == 3 ? to : target == 4 ? pr : ans;
            ids[index % ids.length] ^= mask32;
        }

        vm.expectRevert();
        settlement.verifyAndSettle(modelId, segs, ti, to, pr, ans, proof);
    }
}
