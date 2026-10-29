// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {StubSp1Gateway} from "./StubSp1Gateway.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";

/// End-to-end TinyLlama-1.1B 23-segment settlement through the dev verifier and the SP1 adapter.
contract TinyLlamaSettlementTest is TinyLlamaBase {
    function testEndToEndTinyLlama23SegmentDevSettlement() public {
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        // runId, promptHash and answerHash from SPEC.md §1.4
        bytes32 specRunId = 0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28;
        vm.expectEmit(address(settlement));
        emit VerifiableInferenceSettlement.InferenceVerified(
            specRunId,
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.PUBLIC_DIGEST,
            0xb4d5730f21772191057fb4600dc73dfdf0350fab6996c7b12d1d1ca7b83b9ec1,
            0xaed36ba9347820959ae88d81e8c45a733e404b199fb3c12a463e2301253934c8,
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
        assertEq(runId, specRunId);
        assertTrue(settlement.settledRuns(runId));
    }

    function testEndToEndTinyLlama23SegmentSp1AdapterSettlement() public {
        bytes32 vkey = keccak256("sash-segment-aggregator-elf-v1");
        StubSp1Gateway gateway = _deployStubGateway(vkey);
        Sp1SashSegmentVerifier sp1Adapter = _deploySp1Adapter(address(gateway), vkey);
        VerifiableInferenceSettlement prodSettlement = _deploySettlement(address(sp1Adapter), false);

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
}
