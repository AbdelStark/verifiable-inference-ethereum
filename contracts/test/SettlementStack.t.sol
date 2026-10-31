// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {SettlementStack, TINYLLAMA_RUN_ID} from "../script/SettlementStack.sol";

/// An adapter whose `verifierKind` changes after the settlement's constructor read it: the only way to reach
/// `WrongVerifierKind` past `DevVerifierAllowed`, since a settlement with `allowDevVerifier = false` refuses a
/// `DEV_TRANSCRIPT_ONLY` verifier at deployment.
contract KindSwitchingAdapter {
    address public immutable SP1_VERIFIER;
    bytes32 public immutable SASH_AGGREGATOR_VKEY;
    SashTypes.VerifierKind public verifierKind = SashTypes.VerifierKind.ZKVM_PLONK;

    constructor(address sp1Verifier, bytes32 vkey) {
        SP1_VERIFIER = sp1Verifier;
        SASH_AGGREGATOR_VKEY = vkey;
    }

    function setKind(SashTypes.VerifierKind kind) external {
        verifierKind = kind;
    }
}

/// `SettlementStack._checkStack`, the post-deploy checks `DeployTestnet` and `DeployAnvilSp1` run after their
/// broadcast: a stack settled locally over the v6.1.0 `SP1VerifierPlonk` compiled from the submodule passes them,
/// and each mismatch reverts with its named error. No RPC is needed.
contract SettlementStackChecksTest is Test, SettlementStack {
    address internal sp1Verifier;
    bytes32 internal programVKey;
    bytes internal proofBytes;
    Stack internal stack;

    function setUp() public {
        sp1Verifier = vm.deployCode("SP1VerifierPlonk.sol:SP1Verifier");
        (programVKey, proofBytes) = _readPlonkFixture();
        stack = _deployAndSettle(address(this), sp1Verifier, programVKey, proofBytes);
    }

    /// External, so that `vm.expectRevert` sees the revert one call below it.
    function checkStack(Stack memory s, address owner, address verifier, bytes32 vkey) external view {
        _checkStack(s, owner, verifier, vkey);
    }

    function _registry(uint16 numSegments, uint16 seqLen, uint32 eosTokenId, bytes32 manifestRootsHash)
        internal
        returns (SashModelRegistry r)
    {
        r = SashModelRegistry(vm.deployCode("SashModelRegistry.sol:SashModelRegistry", abi.encode(address(this))));
        r.enrollModel(TinyLlamaFixture.MODEL_ID, numSegments, seqLen, eosTokenId, manifestRootsHash);
    }

    function _settlement(address registry, address adapter, bool allowDevVerifier)
        internal
        returns (VerifiableInferenceSettlement)
    {
        return VerifiableInferenceSettlement(
            vm.deployCode(
                "VerifiableInferenceSettlement.sol:VerifiableInferenceSettlement",
                abi.encode(registry, adapter, allowDevVerifier)
            )
        );
    }

    function testPassesOnTheSettledStack() public view {
        this.checkStack(stack, address(this), sp1Verifier, programVKey);
        assertEq(stack.runId, TINYLLAMA_RUN_ID);
    }

    function testRejectsWrongOwner() public {
        address other = makeAddr("other");
        vm.expectRevert(abi.encodeWithSelector(RegistryOwnerMismatch.selector, address(this), other));
        this.checkStack(stack, other, sp1Verifier, programVKey);
    }

    /// Each manifest field read back: a registry differing from the fixture in exactly one of them.
    function testRejectsEachManifestFieldMismatch() public {
        SashModelRegistry[4] memory registries = [
            _registry(
                TinyLlamaFixture.NUM_SEGMENTS + 1,
                TinyLlamaFixture.SEQ_LEN,
                TinyLlamaFixture.EOS_TOKEN_ID,
                TinyLlamaFixture.MANIFEST_ROOTS_HASH
            ),
            _registry(
                TinyLlamaFixture.NUM_SEGMENTS,
                TinyLlamaFixture.SEQ_LEN + 1,
                TinyLlamaFixture.EOS_TOKEN_ID,
                TinyLlamaFixture.MANIFEST_ROOTS_HASH
            ),
            _registry(
                TinyLlamaFixture.NUM_SEGMENTS,
                TinyLlamaFixture.SEQ_LEN,
                TinyLlamaFixture.EOS_TOKEN_ID + 1,
                TinyLlamaFixture.MANIFEST_ROOTS_HASH
            ),
            _registry(
                TinyLlamaFixture.NUM_SEGMENTS,
                TinyLlamaFixture.SEQ_LEN,
                TinyLlamaFixture.EOS_TOKEN_ID,
                TinyLlamaFixture.MANIFEST_ROOTS_HASH ^ bytes32(uint256(1))
            )
        ];
        Stack memory s = stack;
        for (uint256 i = 0; i < registries.length; i++) {
            s.registry = registries[i];
            vm.expectRevert(ManifestMismatch.selector);
            this.checkStack(s, address(this), sp1Verifier, programVKey);
        }
    }

    function testRejectsDeactivatedModel() public {
        stack.registry.setModelActive(TinyLlamaFixture.MODEL_ID, false);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.ModelNotActive.selector, TinyLlamaFixture.MODEL_ID));
        this.checkStack(stack, address(this), sp1Verifier, programVKey);
    }

    function testRejectsWrongSp1Verifier() public {
        address other = makeAddr("other");
        vm.expectRevert(abi.encodeWithSelector(AdapterVerifierMismatch.selector, sp1Verifier, other));
        this.checkStack(stack, address(this), other, programVKey);
    }

    function testRejectsWrongVKey() public {
        bytes32 other = programVKey ^ bytes32(uint256(1));
        vm.expectRevert(abi.encodeWithSelector(AdapterVKeyMismatch.selector, programVKey, other));
        this.checkStack(stack, address(this), sp1Verifier, other);
    }

    /// A registry, or an adapter, other than the one the settlement is wired to.
    function testRejectsSettlementWiredElsewhere() public {
        Stack memory s = stack;
        s.registry = _registry(
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            TinyLlamaFixture.EOS_TOKEN_ID,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        vm.expectRevert(SettlementWiringMismatch.selector);
        this.checkStack(s, address(this), sp1Verifier, programVKey);

        s = stack;
        s.adapter = Sp1SashSegmentVerifier(
            vm.deployCode("Sp1SashSegmentVerifier.sol:Sp1SashSegmentVerifier", abi.encode(sp1Verifier, programVKey))
        );
        vm.expectRevert(SettlementWiringMismatch.selector);
        this.checkStack(s, address(this), sp1Verifier, programVKey);
    }

    function testRejectsDevVerifierAllowed() public {
        Stack memory s = stack;
        s.settlement = _settlement(address(s.registry), address(s.adapter), true);
        vm.expectRevert(DevVerifierAllowed.selector);
        this.checkStack(s, address(this), sp1Verifier, programVKey);
    }

    function testRejectsWrongVerifierKind() public {
        KindSwitchingAdapter adapter = new KindSwitchingAdapter(sp1Verifier, programVKey);
        Stack memory s = stack;
        s.adapter = Sp1SashSegmentVerifier(address(adapter));
        s.settlement = _settlement(address(s.registry), address(adapter), false);
        adapter.setKind(SashTypes.VerifierKind.DEV_TRANSCRIPT_ONLY);
        vm.expectRevert(abi.encodeWithSelector(WrongVerifierKind.selector, SashTypes.VerifierKind.DEV_TRANSCRIPT_ONLY));
        this.checkStack(s, address(this), sp1Verifier, programVKey);
    }

    function testRejectsWrongRunId() public {
        Stack memory s = stack;
        s.runId = TINYLLAMA_RUN_ID ^ bytes32(uint256(1));
        vm.expectRevert(abi.encodeWithSelector(RunIdMismatch.selector, s.runId, TINYLLAMA_RUN_ID));
        this.checkStack(s, address(this), sp1Verifier, programVKey);
    }

    /// A settlement correctly wired to the same registry and adapter that never settled the run.
    function testRejectsUnsettledRun() public {
        Stack memory s = stack;
        s.settlement = _settlement(address(s.registry), address(s.adapter), false);
        vm.expectRevert(abi.encodeWithSelector(RunNotSettled.selector, TINYLLAMA_RUN_ID));
        this.checkStack(s, address(this), sp1Verifier, programVKey);
    }
}
