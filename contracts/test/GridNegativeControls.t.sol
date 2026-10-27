// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// Grid-level negative controls (`SPEC.md` §2.1): each mutation is submitted with a dev proof for its own `publicDigest`, so the revert comes from the grid check.
contract GridNegativeControlsTest is TinyLlamaBase {
    function testNegative01_ForgedModelRootReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[7].modelRoot ^= bytes32(uint256(1));
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(
            abi.encodeWithSelector(
                SashTypes.ManifestRootMismatch.selector,
                this.externalRootsHash(segs),
                TinyLlamaFixture.MANIFEST_ROOTS_HASH
            )
        );
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            segs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative02_ForgedPreRootReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[0].preRoot ^= bytes32(uint256(1));
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(
            abi.encodeWithSelector(
                SashTypes.ManifestRootMismatch.selector,
                this.externalRootsHash(segs),
                TinyLlamaFixture.MANIFEST_ROOTS_HASH
            )
        );
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            segs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative03_OutOfOrderSegmentIndexReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[5].segmentIndex = 6;
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.InvalidSegmentIndex.selector, 5, 6));
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            segs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative04_BrokenResidualBoundaryLinkReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[10].residualOutRoot ^= bytes32(uint256(0xff));
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.ResidualBoundaryMismatch.selector, 10));
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            segs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative05_HeadNonZeroResidualOutRootReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[22].residualOutRoot = bytes32(uint256(1));
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(SashTypes.HeadMustNotCarryOutRoot.selector);
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            segs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative10_PromptMismatchReverts() public {
        uint32[] memory pr = TinyLlamaFixture.promptIds();
        pr[4] = 9999;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.PromptTokenMismatch.selector, 4));
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            pr,
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative11_BrokenAutoregressiveLoopReverts() public {
        uint32[] memory ti = TinyLlamaFixture.tokIn();
        ti[27] = 1111;
        bytes memory proof =
            _devProofForModel(TinyLlamaFixture.MODEL_ID, TinyLlamaFixture.segments(), ti, TinyLlamaFixture.tokOut());
        vm.expectRevert(
            abi.encodeWithSelector(
                SashTypes.AutoregressiveLoopMismatch.selector, 27 - TinyLlamaFixture.promptIds().length
            )
        );
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            ti,
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative12_MissingEosReverts() public {
        bytes32 altModel = keccak256("alt-eos-model");
        registry.enrollModel(
            altModel, TinyLlamaFixture.NUM_SEGMENTS, TinyLlamaFixture.SEQ_LEN, 999, TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        bytes memory proof = _devProofForModel(altModel, segs, TinyLlamaFixture.tokIn(), TinyLlamaFixture.tokOut());
        vm.expectRevert(
            abi.encodeWithSelector(SashTypes.MissingEosTermination.selector, TinyLlamaFixture.EOS_TOKEN_ID, 999)
        );
        settlement.verifyAndSettle(
            altModel,
            segs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative13_AnswerMismatchReverts() public {
        uint32[] memory ans = TinyLlamaFixture.answerIds();
        ans[3] = 777;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.AnswerTokenMismatch.selector, 3));
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            ans,
            proof
        );
    }

    function testNegative16_TruncatedSegmentArrayReverts() public {
        SashTypes.SegmentClaim[] memory full = TinyLlamaFixture.segments();
        SashTypes.SegmentClaim[] memory shortSegs = new SashTypes.SegmentClaim[](22);
        for (uint256 i = 0; i < 22; ++i) {
            shortSegs[i] = full[i];
        }
        bytes memory proof = _devProofFor(shortSegs);
        vm.expectRevert(
            abi.encodeWithSelector(SashTypes.InvalidSegmentCount.selector, 22, TinyLlamaFixture.NUM_SEGMENTS)
        );
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            shortSegs,
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }
}
