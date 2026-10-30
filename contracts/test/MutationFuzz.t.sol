// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";

/// Single-field mutation fuzz control (`SPEC.md` §2.1, row F), on the SP1 path: the adapter on the real v6.1.0
/// `SP1VerifierPlonk` and `allowDevVerifier = false`.
contract MutationFuzzTest is TinyLlamaBase {
    bytes internal plonkProof;
    VerifiableInferenceSettlement internal prodSettlement;

    function setUp() public override {
        super.setUp();
        bytes32 programVKey;
        (programVKey,, plonkProof) = _plonkFixture();
        (, prodSettlement) = _deployPlonkSettlement(programVKey);
    }

    /// Flips bits in one field of the valid run and submits it with the committed PLONK proof
    /// (`SPEC.md` §2.1, row F). The grid checks nothing that binds `segments[0].residualInRoot`, so only the
    /// proof, through the `publicDigest` it attests, rejects that mutation.
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
        bytes memory proof = plonkProof;

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
        prodSettlement.verifyAndSettle(modelId, segs, ti, to, pr, ans, proof);
    }
}
