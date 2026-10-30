// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {SP1Verifier as SP1VerifierPlonk} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {SP1Verifier as SP1VerifierGroth16} from "@sp1-contracts/v6.1.0/SP1VerifierGroth16.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {GetterStubPlonkVerifier} from "./GetterStubPlonkVerifier.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";

// The v6.1.0 `SP1VerifierPlonk`'s `VERIFIER_HASH()` and `VK_ROOT()` (`SPEC.md` §1.2, `SECURITY.md` §6).
bytes32 constant PLONK_VERIFIER_HASH = 0x5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac;
bytes32 constant VK_ROOT = 0x002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352;
// The v6.1.0 `SP1VerifierGroth16`'s `VERIFIER_HASH()`, and Sepolia's `V6_1_0_BROKEN_SP1_VERIFIER_PLONK`'s stale
// `VK_ROOT()` (`SECURITY.md` §6).
bytes32 constant GROTH16_VERIFIER_HASH = 0x4388a21c687fdd5f218d7e3d13190cac4c5355818d3605fd5fb811df468ee696;
bytes32 constant BROKEN_VK_ROOT = 0x00995aa7753b437dfe98b936e602e002155914af1922d96e1af288578adc2251;
// Any non-zero `programVKey`: each deployment control reverts before a proof is checked.
bytes32 constant PROGRAM_VKEY = keccak256("sash-aggregator-vkey");

/// `Sp1SashSegmentVerifier`'s binding to the real v6.1.0 `SP1VerifierPlonk`, compiled from the submodule
/// (`SECURITY.md` §6), and the deployment controls 28-30 (`SPEC.md` §2.1). The SP1 contracts are deployed with
/// `deployCode`, which keeps their initcode out of the test contracts' `code_size_limit`; the adapter with `new`,
/// so that an expected constructor revert stays inside the test.
contract Sp1VerifierBindingTest is Test {
    SP1VerifierPlonk internal plonk;

    function setUp() public {
        vm.chainId(31337);
        plonk = SP1VerifierPlonk(deployCode("SP1VerifierPlonk.sol:SP1Verifier"));
    }

    function testPinnedPlonkVerifier() public view {
        assertEq(plonk.VERSION(), "v6.1.0");
        assertEq(plonk.VERIFIER_HASH(), PLONK_VERIFIER_HASH);
        assertEq(plonk.VK_ROOT(), VK_ROOT);
    }

    function testAdapterAcceptsPinnedPlonkVerifier() public {
        Sp1SashSegmentVerifier adapter = new Sp1SashSegmentVerifier(address(plonk), PROGRAM_VKEY);
        assertEq(adapter.SP1_PLONK_VERIFIER_HASH(), PLONK_VERIFIER_HASH);
        assertEq(adapter.SP1_VK_ROOT(), VK_ROOT);
        assertEq(address(adapter.SP1_VERIFIER()), address(plonk));
        assertEq(adapter.SASH_AGGREGATOR_VKEY(), PROGRAM_VKEY);
    }

    function testAdapterRejectsZeroVerifierAndZeroVKey() public {
        vm.expectRevert(Sp1SashSegmentVerifier.ZeroVerifierAddress.selector);
        new Sp1SashSegmentVerifier(address(0), PROGRAM_VKEY);
        vm.expectRevert(Sp1SashSegmentVerifier.ZeroProgramVKey.selector);
        new Sp1SashSegmentVerifier(address(plonk), bytes32(0));
    }

    /// The gateway has no `VERIFIER_HASH()` and no fallback, and an address without code returns nothing to
    /// decode: both revert with empty returndata.
    function testNegative28_GatewayRejectedAtDeploy() public {
        SP1VerifierGateway gateway =
            SP1VerifierGateway(deployCode("SP1VerifierGateway.sol:SP1VerifierGateway", abi.encode(address(this))));
        assertEq(gateway.owner(), address(this));
        _assertBareRevert(address(gateway));
        _assertBareRevert(makeAddr("no-code"));
    }

    function testNegative29_Groth16VerifierRejectedAtDeploy() public {
        SP1VerifierGroth16 groth16 = SP1VerifierGroth16(deployCode("SP1VerifierGroth16.sol:SP1Verifier"));
        assertEq(groth16.VERSION(), "v6.1.0");
        vm.expectRevert(
            abi.encodeWithSelector(Sp1SashSegmentVerifier.WrongSp1Verifier.selector, GROTH16_VERIFIER_HASH, VK_ROOT)
        );
        new Sp1SashSegmentVerifier(address(groth16), PROGRAM_VKEY);
    }

    function testNegative30_BrokenVkRootPlonkVerifierRejectedAtDeploy() public {
        GetterStubPlonkVerifier broken = new GetterStubPlonkVerifier(PLONK_VERIFIER_HASH, BROKEN_VK_ROOT);
        vm.expectRevert(
            abi.encodeWithSelector(
                Sp1SashSegmentVerifier.WrongSp1Verifier.selector, PLONK_VERIFIER_HASH, BROKEN_VK_ROOT
            )
        );
        new Sp1SashSegmentVerifier(address(broken), PROGRAM_VKEY);
    }

    /// All 32 bytes of `VERIFIER_HASH()` are checked, not only the 4-byte selector the verifier reads from
    /// `proofBytes`.
    function testAdapterRejectsVerifierHashDifferingPastSelector() public {
        bytes32 verifierHash = PLONK_VERIFIER_HASH ^ bytes32(uint256(1));
        assertEq(bytes4(verifierHash), bytes4(PLONK_VERIFIER_HASH));
        GetterStubPlonkVerifier stub = new GetterStubPlonkVerifier(verifierHash, VK_ROOT);
        vm.expectRevert(abi.encodeWithSelector(Sp1SashSegmentVerifier.WrongSp1Verifier.selector, verifierHash, VK_ROOT));
        new Sp1SashSegmentVerifier(address(stub), PROGRAM_VKEY);
    }

    function _assertBareRevert(address sp1Verifier) internal {
        try new Sp1SashSegmentVerifier(sp1Verifier, PROGRAM_VKEY) {
            fail();
        } catch (bytes memory reason) {
            assertEq(reason.length, 0);
        }
    }
}

/// The settlement controls on the committed PLONK proof (`SPEC.md` §2.1): 22-27, through `verifyAndSettle` with
/// `allowDevVerifier = false` and the adapter on the real v6.1.0 `SP1VerifierPlonk`. Each is the valid
/// settlement with one mutation.
contract Sp1PlonkSettlementControlsTest is TinyLlamaBase {
    // `sp1/segment-guest.vkey`: a real SP1 vkey, not the aggregator's, whose top byte is `0x00`, so that it stays
    // below the BN254 scalar field `r` and reaches the pairing check.
    bytes32 internal constant SEGMENT_GUEST_VKEY = 0x003b73b0c998606408f61a261098a22a47e5edcbad107157e13763ef90c4997c;
    // The BN254 scalar field modulus, the bound `PlonkVerifier` checks each public input against.
    uint256 internal constant BN254_R = 21888242871839275222246405745257275088548364400416034343698204186575808495617;

    bytes32 internal programVKey;
    bytes internal proofBytes;
    VerifiableInferenceSettlement internal prodSettlement;

    function setUp() public override {
        super.setUp();
        (programVKey,, proofBytes) = _plonkFixture();
        (, prodSettlement) = _deployPlonkSettlement(programVKey);
    }

    function testNegative22_AdapterPinnedToSegmentGuestVKeyReverts() public {
        assertTrue(SEGMENT_GUEST_VKEY != programVKey);
        assertLt(uint256(SEGMENT_GUEST_VKEY), BN254_R);
        (, VerifiableInferenceSettlement wrongSettlement) = _deployPlonkSettlement(SEGMENT_GUEST_VKEY);
        vm.expectRevert(SP1VerifierPlonk.InvalidProof.selector);
        _settle(wrongSettlement, TinyLlamaFixture.segments(), TinyLlamaFixture.tokOut(), proofBytes);
    }

    function testNegative23_WrongVerifierSelectorReverts() public {
        bytes memory proof = proofBytes;
        proof[0] = 0x00;
        vm.expectRevert(
            abi.encodeWithSelector(
                SP1VerifierPlonk.WrongVerifierSelector.selector, bytes4(0x00093a2f), bytes4(0x5a093a2f)
            )
        );
        _settle(prodSettlement, TinyLlamaFixture.segments(), TinyLlamaFixture.tokOut(), proof);
    }

    function testNegative24_NonZeroExitCodeReverts() public {
        bytes memory proof = proofBytes;
        _setWord(proof, 4, bytes32(uint256(1)));
        vm.expectRevert(SP1VerifierPlonk.InvalidExitCode.selector);
        _settle(prodSettlement, TinyLlamaFixture.segments(), TinyLlamaFixture.tokOut(), proof);
    }

    function testNegative25_WrongVkRootReverts() public {
        bytes memory proof = proofBytes;
        _setWord(proof, 36, BROKEN_VK_ROOT);
        vm.expectRevert(SP1VerifierPlonk.InvalidVkRoot.selector);
        _settle(prodSettlement, TinyLlamaFixture.segments(), TinyLlamaFixture.tokOut(), proof);
    }

    /// The low byte of `L(zeta)`, the first opening of the PLONK proof (`proofBytes[100 + 0x180 + 31]`): the
    /// opening stays below `r`, so the verifier runs to the pairing check, which fails.
    function testNegative26_FlippedPlonkProofByteReverts() public {
        bytes memory proof = proofBytes;
        proof[100 + 0x180 + 31] ^= 0x01;
        vm.expectRevert(SP1VerifierPlonk.InvalidProof.selector);
        _settle(prodSettlement, TinyLlamaFixture.segments(), TinyLlamaFixture.tokOut(), proof);
    }

    /// The proof check runs before the grid (`VerifiableInferenceSettlement.verifyAndSettle`), so the recomputed
    /// `publicDigest`, which no longer matches the one the proof attests, is what rejects each mutation.
    function testNegative27_ClaimOrTokenMutationFailsPlonkProof() public {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();
        segs[3].fixedHash ^= bytes32(uint256(1));
        vm.expectRevert(SP1VerifierPlonk.InvalidProof.selector);
        _settle(prodSettlement, segs, TinyLlamaFixture.tokOut(), proofBytes);

        uint32[] memory to = TinyLlamaFixture.tokOut();
        to[31] ^= 1;
        vm.expectRevert(SP1VerifierPlonk.InvalidProof.selector);
        _settle(prodSettlement, TinyLlamaFixture.segments(), to, proofBytes);
    }

    function _setWord(bytes memory proof, uint256 offset, bytes32 word) internal pure {
        for (uint256 i = 0; i < 32; i++) {
            proof[offset + i] = word[i];
        }
    }

    function _settle(
        VerifiableInferenceSettlement target,
        SashTypes.SegmentClaim[] memory segs,
        uint32[] memory tokOut,
        bytes memory proof
    ) internal {
        target.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            segs,
            TinyLlamaFixture.tokIn(),
            tokOut,
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
    }
}
