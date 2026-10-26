// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {M31} from "../src/field/M31.sol";
import {QM31} from "../src/field/QM31.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {DevTranscriptSegmentVerifier} from "../src/sash/DevTranscriptSegmentVerifier.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashGridVerifier} from "../src/sash/SashGridVerifier.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

contract StubSp1Gateway is ISP1Verifier {
    bytes32 public immutable EXPECTED_VKEY;
    error BadSp1Proof();

    constructor(bytes32 vkey) {
        EXPECTED_VKEY = vkey;
    }

    function verifyProof(bytes32 programVKey, bytes calldata publicValues, bytes calldata proofBytes)
        external
        view
        override
    {
        bytes32 expectedProof = keccak256(abi.encodePacked(EXPECTED_VKEY, publicValues));
        if (programVKey != EXPECTED_VKEY || proofBytes.length != 32 || bytes32(proofBytes) != expectedProof) {
            revert BadSp1Proof();
        }
    }
}

contract TinyLlamaSettlementTest is Test {
    SashModelRegistry internal registry;
    DevTranscriptSegmentVerifier internal devVerifier;
    VerifiableInferenceSettlement internal settlement;

    function setUp() public {
        vm.chainId(31337);
        registry = new SashModelRegistry(address(this));
        registry.enrollModel(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            TinyLlamaFixture.EOS_TOKEN_ID,
            TinyLlamaFixture.B0_REL_DRAWS,
            TinyLlamaFixture.HEAD_REL_DRAWS,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        devVerifier = new DevTranscriptSegmentVerifier();
        settlement = new VerifiableInferenceSettlement(address(registry), address(devVerifier), true);
    }

    function testEndToEndTinyLlama23SegmentDevSettlement() public {
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        bytes32 runId = settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
        assertTrue(settlement.settledRuns(runId));
    }

    function testEndToEndTinyLlama23SegmentSp1AdapterSettlement() public {
        bytes32 vkey = keccak256("sash-segment-aggregator-elf-v1");
        StubSp1Gateway gateway = new StubSp1Gateway(vkey);
        Sp1SashSegmentVerifier sp1Adapter = new Sp1SashSegmentVerifier(address(gateway), vkey);
        VerifiableInferenceSettlement prodSettlement =
            new VerifiableInferenceSettlement(address(registry), address(sp1Adapter), false);

        vm.chainId(11155111); // Sepolia chain ID
        bytes memory proof =
            abi.encodePacked(keccak256(abi.encodePacked(vkey, abi.encodePacked(TinyLlamaFixture.PUBLIC_DIGEST))));
        bytes32 runId = prodSettlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
        assertTrue(prodSettlement.settledRuns(runId));
    }

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

    function testNegative06_MutatedTokInFailsHornerBinding() public {
        uint32[] memory ti = TinyLlamaFixture.tokIn();
        uint32[] memory pr = TinyLlamaFixture.promptIds();
        ti[0] = 530;
        pr[0] = 530;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(SashTypes.EmbedTokenHornerMismatch.selector);
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            ti,
            TinyLlamaFixture.tokOut(),
            pr,
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative07_MutatedTokOutFailsHornerBinding() public {
        uint32[] memory to = TinyLlamaFixture.tokOut();
        to[0] ^= 1;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(SashTypes.HeadTokenHornerMismatch.selector);
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            to,
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative08_MutatedTraceRootShiftsFiatShamirChallenge() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[0].traceRoot ^= bytes32(uint256(1));
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(SashTypes.EmbedTokenHornerMismatch.selector);
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

    function testNegative09_WrongRelationDrawsReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[22].relationDraws = 25;
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(
            abi.encodeWithSelector(
                SashTypes.RelationDrawCountMismatch.selector, 22, 25, TinyLlamaFixture.HEAD_REL_DRAWS
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
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
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
            altModel,
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            999,
            TinyLlamaFixture.B0_REL_DRAWS,
            TinyLlamaFixture.HEAD_REL_DRAWS,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        bytes memory proof = _devProofForModel(altModel, segs);
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

    function testNegative14_NonCanonicalM31InHornerClaimReverts() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[1].tokenHornerClaim = QM31.Elem(0x7fffffff, 0, 0, 0);
        bytes memory proof = _devProofFor(segs);
        vm.expectRevert(abi.encodeWithSelector(M31.NonCanonicalM31.selector, 0x7fffffff));
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

    function testNegative15_NonCanonicalPromptTokenReverts() public {
        uint32[] memory pr = TinyLlamaFixture.promptIds();
        pr[0] = 0x7fffffff;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(abi.encodeWithSelector(M31.NonCanonicalM31.selector, 0x7fffffff));
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

    function testNegative17_InvalidProofBytesReverts() public {
        vm.expectRevert(SashTypes.InvalidDevProofTag.selector);
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            hex"deadbeef"
        );
    }

    function testNegative18_WrongSp1VKeyProofReverts() public {
        bytes32 vkey = keccak256("sash-segment-aggregator-elf-v1");
        StubSp1Gateway gateway = new StubSp1Gateway(vkey);
        Sp1SashSegmentVerifier sp1Adapter = new Sp1SashSegmentVerifier(address(gateway), vkey);
        VerifiableInferenceSettlement prodSettlement =
            new VerifiableInferenceSettlement(address(registry), address(sp1Adapter), false);

        bytes32 wrongVkey = keccak256("wrong-elf");
        bytes memory badProof =
            abi.encodePacked(keccak256(abi.encodePacked(wrongVkey, abi.encodePacked(TinyLlamaFixture.PUBLIC_DIGEST))));
        vm.expectRevert(StubSp1Gateway.BadSp1Proof.selector);
        prodSettlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            badProof
        );
    }

    function testNegative19_DevVerifierBlockedOnSepoliaAndWhenDisallowed() public {
        vm.expectRevert(SashTypes.DevVerifierForbidden.selector);
        new VerifiableInferenceSettlement(address(registry), address(devVerifier), false);

        vm.chainId(11155111);
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.DevChainOnly.selector, 11155111));
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testNegative20_DuplicateRunReplayReverts() public {
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        bytes32 runId = settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
        vm.expectRevert(abi.encodeWithSelector(SashTypes.RunAlreadySettled.selector, runId));
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }

    function testRegistryRejectsZeroOwner() public {
        vm.expectRevert(SashTypes.ZeroOwner.selector);
        new SashModelRegistry(address(0));
    }

    function _devProofFor(SashTypes.SegmentClaim[] memory segs) internal view returns (bytes memory) {
        return _devProofForModel(TinyLlamaFixture.MODEL_ID, segs);
    }

    function _devProofForModel(bytes32 modelId, SashTypes.SegmentClaim[] memory segs)
        internal
        view
        returns (bytes memory)
    {
        bytes32 digest = this.externalDigest(modelId, segs);
        return devVerifier.computeDevProof(digest);
    }

    function externalDigest(bytes32 modelId, SashTypes.SegmentClaim[] calldata segs) external pure returns (bytes32) {
        return SashGridVerifier.computePublicDigest(modelId, segs);
    }

    function externalRootsHash(SashTypes.SegmentClaim[] calldata segs) external pure returns (bytes32) {
        return SashGridVerifier.computeManifestRootsHash(segs);
    }
}
