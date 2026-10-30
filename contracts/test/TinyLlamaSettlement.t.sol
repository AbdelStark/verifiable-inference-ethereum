// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";

// `SPEC.md` §1.4
bytes32 constant SPEC_PUBLIC_DIGEST = 0xcf52e19e82f82680f0098ab435b52e22512f3d634bd7a947d32abc76d067b3ef;
bytes32 constant SPEC_PROMPT_HASH = 0xb4d5730f21772191057fb4600dc73dfdf0350fab6996c7b12d1d1ca7b83b9ec1;
bytes32 constant SPEC_ANSWER_HASH = 0xaed36ba9347820959ae88d81e8c45a733e404b199fb3c12a463e2301253934c8;
bytes32 constant SPEC_RUN_ID = 0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28;
// `sp1/aggregator-guest.vkey`
bytes32 constant AGGREGATOR_VKEY = 0x00eb19e4a03314f2190169c9b8454a2223f1afff657ceba3642a1fc1fe97747c;

/// End-to-end TinyLlama-1.1B 23-segment settlement through the dev verifier and through the SP1 adapter on the
/// real PLONK proof.
contract TinyLlamaSettlementTest is TinyLlamaBase {
    function testEndToEndTinyLlama23SegmentDevSettlement() public {
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        vm.expectEmit(address(settlement));
        emit VerifiableInferenceSettlement.InferenceVerified(
            SPEC_RUN_ID,
            TinyLlamaFixture.MODEL_ID,
            SPEC_PUBLIC_DIGEST,
            SPEC_PROMPT_HASH,
            SPEC_ANSWER_HASH,
            SashTypes.VerifierKind.DEV_TRANSCRIPT_ONLY
        );
        bytes32 runId = settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
        assertEq(runId, SPEC_RUN_ID);
        assertTrue(settlement.settledRuns(runId));
    }

    /// The committed PLONK proof settles through the real v6.1.0 `SP1VerifierPlonk` with
    /// `allowDevVerifier = false`. The fixture's `publicValues` are `SPEC.md` §1.4's `publicDigest` and its
    /// `programVKey` the aggregator's, `sp1/aggregator-guest.vkey`.
    function testEndToEndTinyLlama23SegmentPlonkSettlement() public {
        (bytes32 programVKey, bytes memory publicValues, bytes memory proofBytes) = _plonkFixture();
        assertEq(programVKey, AGGREGATOR_VKEY);
        assertEq(publicValues, abi.encodePacked(SPEC_PUBLIC_DIGEST));
        assertEq(proofBytes.length, 964);
        (Sp1SashSegmentVerifier adapter, VerifiableInferenceSettlement prodSettlement) =
            _deployPlonkSettlement(programVKey);
        assertEq(uint8(adapter.verifierKind()), uint8(SashTypes.VerifierKind.ZKVM_PLONK));
        assertFalse(prodSettlement.ALLOW_DEV_VERIFIER());

        vm.expectEmit(address(prodSettlement));
        emit VerifiableInferenceSettlement.InferenceVerified(
            SPEC_RUN_ID,
            TinyLlamaFixture.MODEL_ID,
            SPEC_PUBLIC_DIGEST,
            SPEC_PROMPT_HASH,
            SPEC_ANSWER_HASH,
            SashTypes.VerifierKind.ZKVM_PLONK
        );
        bytes32 runId = prodSettlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proofBytes
        );
        assertEq(runId, SPEC_RUN_ID);
        assertTrue(prodSettlement.settledRuns(runId));
    }
}
