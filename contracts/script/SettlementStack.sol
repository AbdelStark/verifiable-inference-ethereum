// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {CommonBase} from "forge-std/Base.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";

// The TinyLlama run's `runId` (`SPEC.md` §1.4).
bytes32 constant TINYLLAMA_RUN_ID = 0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28;

/// @title SettlementStack
/// @notice The production SASH settlement stack over a given SP1 v6.1.0 `SP1VerifierPlonk`, shared by
///         `DeployTestnet`, `DeployAnvilSp1` and `HoodiForkTest`, so the stack code the tests run is the code that
///         is broadcast: the registry with `TinyLlama-1.1B-Chat-v1.0` (`y-attnout-w12`) enrolled,
///         `Sp1SashSegmentVerifier`, `VerifiableInferenceSettlement` with `allowDevVerifier = false`, and the
///         settle of the committed PLONK proof (`contracts/fixtures/tinyllama.plonk.json`).
/// @dev Contracts are deployed with `vm.deployCode`, never `new`, so an inheriting test or script does not embed
///      their initcode and stays under `code_size_limit`. Under `vm.startBroadcast()` each one is broadcast as a
///      `CREATE` from the sender, and the broadcast JSON names it.
abstract contract SettlementStack is CommonBase {
    struct Stack {
        SashModelRegistry registry;
        Sp1SashSegmentVerifier adapter;
        VerifiableInferenceSettlement settlement;
        bytes32 runId;
    }

    error RegistryOwnerMismatch(address got, address expected);
    error ManifestMismatch();
    error AdapterVerifierMismatch(address got, address expected);
    error AdapterVKeyMismatch(bytes32 got, bytes32 expected);
    error SettlementWiringMismatch();
    error DevVerifierAllowed();
    error WrongVerifierKind(SashTypes.VerifierKind got);
    error RunIdMismatch(bytes32 got, bytes32 expected);
    error RunNotSettled(bytes32 runId);

    /// `programVKey` and `proofBytes` of the committed PLONK proof.
    function _readPlonkFixture() internal view returns (bytes32 programVKey, bytes memory proofBytes) {
        string memory json = vm.readFile(string.concat(vm.projectRoot(), "/fixtures/tinyllama.plonk.json"));
        programVKey = vm.parseJsonBytes32(json, ".programVKey");
        proofBytes = vm.parseJsonBytes(json, ".proofBytes");
    }

    /// Deploys the stack and settles the TinyLlama run: four transactions under a broadcast, then the settle.
    /// @param owner The registry's immutable `OWNER`. It must be the address the calls come from, since
    ///        `enrollModel` is `onlyOwner`: `msg.sender` inside a script's broadcast, `address(this)` in a test.
    function _deployAndSettle(address owner, address sp1Verifier, bytes32 programVKey, bytes memory proofBytes)
        internal
        returns (Stack memory s)
    {
        s.registry = SashModelRegistry(vm.deployCode("SashModelRegistry.sol:SashModelRegistry", abi.encode(owner)));
        s.registry
            .enrollModel(
                TinyLlamaFixture.MODEL_ID,
                TinyLlamaFixture.NUM_SEGMENTS,
                TinyLlamaFixture.SEQ_LEN,
                TinyLlamaFixture.EOS_TOKEN_ID,
                TinyLlamaFixture.MANIFEST_ROOTS_HASH
            );
        s.adapter = Sp1SashSegmentVerifier(
            vm.deployCode("Sp1SashSegmentVerifier.sol:Sp1SashSegmentVerifier", abi.encode(sp1Verifier, programVKey))
        );
        s.settlement = VerifiableInferenceSettlement(
            vm.deployCode(
                "VerifiableInferenceSettlement.sol:VerifiableInferenceSettlement",
                abi.encode(address(s.registry), address(s.adapter), false)
            )
        );
        s.runId = _settleTinyLlama(s.settlement, proofBytes);
    }

    /// `verifyAndSettle` of the TinyLlama run on `settlement`.
    function _settleTinyLlama(VerifiableInferenceSettlement settlement, bytes memory proofBytes)
        internal
        returns (bytes32)
    {
        return settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proofBytes
        );
    }

    /// The post-deploy checks: every immutable read back, the enrolled manifest, and the settled run.
    function _checkStack(Stack memory s, address owner, address sp1Verifier, bytes32 programVKey) internal view {
        if (s.registry.OWNER() != owner) revert RegistryOwnerMismatch(s.registry.OWNER(), owner);

        // `getManifest` reverts with `ModelNotActive` unless the model is enrolled and active.
        SashTypes.ModelManifest memory m = s.registry.getManifest(TinyLlamaFixture.MODEL_ID);
        if (
            m.numSegments != TinyLlamaFixture.NUM_SEGMENTS || m.seqLen != TinyLlamaFixture.SEQ_LEN
                || m.eosTokenId != TinyLlamaFixture.EOS_TOKEN_ID
                || m.manifestRootsHash != TinyLlamaFixture.MANIFEST_ROOTS_HASH
        ) revert ManifestMismatch();

        if (address(s.adapter.SP1_VERIFIER()) != sp1Verifier) {
            revert AdapterVerifierMismatch(address(s.adapter.SP1_VERIFIER()), sp1Verifier);
        }
        if (s.adapter.SASH_AGGREGATOR_VKEY() != programVKey) {
            revert AdapterVKeyMismatch(s.adapter.SASH_AGGREGATOR_VKEY(), programVKey);
        }

        if (
            address(s.settlement.REGISTRY()) != address(s.registry)
                || address(s.settlement.SEGMENT_VERIFIER()) != address(s.adapter)
        ) revert SettlementWiringMismatch();
        if (s.settlement.ALLOW_DEV_VERIFIER()) revert DevVerifierAllowed();
        SashTypes.VerifierKind kind = s.settlement.SEGMENT_VERIFIER().verifierKind();
        if (kind != SashTypes.VerifierKind.ZKVM_PLONK) revert WrongVerifierKind(kind);

        if (s.runId != TINYLLAMA_RUN_ID) revert RunIdMismatch(s.runId, TINYLLAMA_RUN_ID);
        if (!s.settlement.settledRuns(s.runId)) revert RunNotSettled(s.runId);
    }
}
