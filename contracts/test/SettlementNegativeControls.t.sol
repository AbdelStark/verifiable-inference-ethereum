// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";

/// Settlement-level negative controls (`SPEC.md` §2.1): token binding, proof bytes, verifier gating and replay.
contract SettlementNegativeControlsTest is TinyLlamaBase {
    function testNegative06_MutatedTokInFailsSegmentProof() public {
        uint32[] memory ti = TinyLlamaFixture.tokIn();
        uint32[] memory pr = TinyLlamaFixture.promptIds();
        ti[0] = 530;
        pr[0] = 530;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(SashTypes.InvalidDevProofTag.selector);
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

    function testNegative07_MutatedTokOutFailsSegmentProof() public {
        uint32[] memory to = TinyLlamaFixture.tokOut();
        to[0] ^= 1;
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectRevert(SashTypes.InvalidDevProofTag.selector);
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
}
