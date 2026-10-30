// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {DevTranscriptSegmentVerifier} from "../src/sash/DevTranscriptSegmentVerifier.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashGridVerifier} from "../src/sash/SashGridVerifier.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// Shared dev-mode stack with the TinyLlama manifest enrolled, and the SP1 stack that settles the committed PLONK
/// proof (`SPEC.md` §1.4, `contracts/fixtures/tinyllama.plonk.json`). Contracts are deployed with `deployCode`,
/// so test contracts don't embed their initcode and stay under `code_size_limit`.
abstract contract TinyLlamaBase is Test {
    SashModelRegistry internal registry;
    DevTranscriptSegmentVerifier internal devVerifier;
    VerifiableInferenceSettlement internal settlement;

    function setUp() public virtual {
        vm.chainId(31337);
        registry = SashModelRegistry(deployCode("SashModelRegistry.sol:SashModelRegistry", abi.encode(address(this))));
        registry.enrollModel(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            TinyLlamaFixture.EOS_TOKEN_ID,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        devVerifier =
            DevTranscriptSegmentVerifier(deployCode("DevTranscriptSegmentVerifier.sol:DevTranscriptSegmentVerifier"));
        settlement = _deploySettlement(address(devVerifier), true);
    }

    function _deploySettlement(address segmentVerifier, bool allowDevVerifier)
        internal
        returns (VerifiableInferenceSettlement)
    {
        return VerifiableInferenceSettlement(
            deployCode(
                "VerifiableInferenceSettlement.sol:VerifiableInferenceSettlement",
                abi.encode(address(registry), segmentVerifier, allowDevVerifier)
            )
        );
    }

    /// The committed PLONK proof of the aggregator over the 23 TinyLlama segment proofs.
    function _plonkFixture()
        internal
        view
        returns (bytes32 programVKey, bytes memory publicValues, bytes memory proofBytes)
    {
        string memory json = vm.readFile(string.concat(vm.projectRoot(), "/fixtures/tinyllama.plonk.json"));
        programVKey = vm.parseJsonBytes32(json, ".programVKey");
        publicValues = vm.parseJsonBytes(json, ".publicValues");
        proofBytes = vm.parseJsonBytes(json, ".proofBytes");
    }

    /// The production stack: the v6.1.0 `SP1VerifierPlonk` compiled from the submodule, the adapter pinned to
    /// `programVKey` and a settlement with `allowDevVerifier = false`; no dev verifier is reachable from it.
    function _deployPlonkSettlement(bytes32 programVKey)
        internal
        returns (Sp1SashSegmentVerifier adapter, VerifiableInferenceSettlement prodSettlement)
    {
        adapter = _deploySp1Adapter(deployCode("SP1VerifierPlonk.sol:SP1Verifier"), programVKey);
        prodSettlement = _deploySettlement(address(adapter), false);
    }

    function _deploySp1Adapter(address sp1Verifier, bytes32 vkey) internal returns (Sp1SashSegmentVerifier) {
        return Sp1SashSegmentVerifier(
            deployCode("Sp1SashSegmentVerifier.sol:Sp1SashSegmentVerifier", abi.encode(sp1Verifier, vkey))
        );
    }

    function _devProofFor(SashTypes.SegmentClaim[] memory segs) internal view returns (bytes memory) {
        return _devProofForModel(TinyLlamaFixture.MODEL_ID, segs, TinyLlamaFixture.tokIn(), TinyLlamaFixture.tokOut());
    }

    function _devProofForModel(
        bytes32 modelId,
        SashTypes.SegmentClaim[] memory segs,
        uint32[] memory tokIn,
        uint32[] memory tokOut
    ) internal view returns (bytes memory) {
        bytes32 digest = this.externalDigest(modelId, segs, tokIn, tokOut);
        return devVerifier.computeDevProof(digest);
    }

    function externalDigest(
        bytes32 modelId,
        SashTypes.SegmentClaim[] calldata segs,
        uint32[] calldata tokIn,
        uint32[] calldata tokOut
    ) external pure returns (bytes32) {
        return SashGridVerifier.computePublicDigest(modelId, segs, tokIn, tokOut);
    }

    function externalRootsHash(SashTypes.SegmentClaim[] calldata segs) external pure returns (bytes32) {
        return SashGridVerifier.computeManifestRootsHash(segs);
    }
}
