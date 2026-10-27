// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {StubSp1Gateway} from "./StubSp1Gateway.sol";
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

    function testNegative18_WrongSp1VKeyProofReverts() public {
        bytes32 vkey = keccak256("sash-segment-aggregator-elf-v1");
        StubSp1Gateway gateway = _deployStubGateway(vkey);
        Sp1SashSegmentVerifier sp1Adapter = _deploySp1Adapter(address(gateway), vkey);
        VerifiableInferenceSettlement prodSettlement = _deploySettlement(address(sp1Adapter), false);

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

        // An adapter pinned to the wrong vkey, with a proof the gateway accepts for its own vkey.
        Sp1SashSegmentVerifier wrongAdapter = _deploySp1Adapter(address(gateway), wrongVkey);
        VerifiableInferenceSettlement wrongSettlement = _deploySettlement(address(wrongAdapter), false);
        bytes memory gatewayProof =
            abi.encodePacked(keccak256(abi.encodePacked(vkey, abi.encodePacked(TinyLlamaFixture.PUBLIC_DIGEST))));
        vm.expectRevert(StubSp1Gateway.BadSp1Proof.selector);
        wrongSettlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            gatewayProof
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
}
