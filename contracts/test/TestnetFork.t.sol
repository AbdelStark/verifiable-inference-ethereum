// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Vm} from "forge-std/Vm.sol";
import {SP1Verifier as SP1VerifierPlonk} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {BROKEN_VK_ROOT, PLONK_VERIFIER_HASH, VK_ROOT} from "./Sp1VerifierBinding.t.sol";
import {SPEC_RUN_ID} from "./TinyLlamaSettlement.t.sol";

// `V6_1_0_SP1_VERIFIER_PLONK` on Hoodi, Sepolia and mainnet: the same runtime code on all three.
address constant SP1_VERIFIER_PLONK = 0xc3c6dDDAc8829b233Dc6536Ec024775a57b0AF2A;
bytes32 constant SP1_VERIFIER_PLONK_CODEHASH = 0xa5c9448eec43538d11533b207300fd13277c14c0800d33549a9492e7e8bdf84b;
// Sepolia's `V6_1_0_BROKEN_SP1_VERIFIER_PLONK`: the pinned `VERSION` and `VERIFIER_HASH`, a stale `VK_ROOT`.
address constant BROKEN_SP1_VERIFIER_PLONK = 0xFaa0Ef4fC2d30bca021A982214E375cdeCFcc8f8;
bytes32 constant BROKEN_SP1_VERIFIER_PLONK_CODEHASH =
    0x413a62f179f67c0ca0ec1ee42b7bf4cee2cd1d68b420165ec673876658824465;

/// The live SP1 v6.1.0 PLONK verifier on a public testnet, at a pinned block. Each fork test contract reads its RPC
/// URL from the environment and is skipped when it is unset. The URL must be an archive endpoint that serves state at
/// the pinned block (a pruning one fails `setUp`), e.g. https://hoodi.drpc.org, https://sepolia.gateway.tenderly.co.
/// `setUp` doesn't call `super.setUp()`, whose `vm.chainId(31337)` and dev verifier would make the fork pass as the
/// dev chain: `block.chainid` is the chain's own, which needs `chain_id` unset in `foundry.toml`. No gas is pinned or
/// snapshotted here.
abstract contract TestnetForkTest is TinyLlamaBase {
    bytes32 internal programVKey;
    bytes internal proofBytes;

    function _selectFork(string memory rpcEnv, uint256 forkBlock, uint256 chainId) internal {
        string memory url = vm.envOr(rpcEnv, string(""));
        if (bytes(url).length == 0) {
            vm.skip(true);
            return;
        }
        vm.createSelectFork(url, forkBlock);
        assertEq(block.chainid, chainId);
        assertEq(block.number, forkBlock);
        (programVKey,, proofBytes) = _plonkFixture();
    }

    /// `name` in sp1-contracts' `contracts/deployments/<block.chainid>.json`.
    function _deployment(string memory name) internal view returns (address) {
        string memory json = vm.readFile(
            string.concat(
                vm.projectRoot(), "/lib/sp1-contracts/contracts/deployments/", vm.toString(block.chainid), ".json"
            )
        );
        return vm.parseJsonAddress(json, string.concat(".", name));
    }

    function testForkPinnedSp1VerifierPlonk() public view {
        assertEq(_deployment("V6_1_0_SP1_VERIFIER_PLONK"), SP1_VERIFIER_PLONK);
        assertEq(SP1_VERIFIER_PLONK.codehash, SP1_VERIFIER_PLONK_CODEHASH);
        assertEq(SP1_VERIFIER_PLONK.code.length, 10_125);
        SP1VerifierPlonk live = SP1VerifierPlonk(SP1_VERIFIER_PLONK);
        assertEq(live.VERSION(), "v6.1.0");
        assertEq(live.VERIFIER_HASH(), PLONK_VERIFIER_HASH);
        assertEq(live.VK_ROOT(), VK_ROOT);
    }

    function testForkAdapterBindsLiveSp1VerifierPlonk() public {
        Sp1SashSegmentVerifier adapter = _deploySp1Adapter(SP1_VERIFIER_PLONK, programVKey);
        SP1VerifierPlonk live = SP1VerifierPlonk(SP1_VERIFIER_PLONK);
        assertEq(adapter.SP1_PLONK_VERIFIER_HASH(), live.VERIFIER_HASH());
        assertEq(adapter.SP1_VK_ROOT(), live.VK_ROOT());
        assertEq(address(adapter.SP1_VERIFIER()), SP1_VERIFIER_PLONK);
        assertEq(adapter.SASH_AGGREGATOR_VKEY(), programVKey);
    }
}

/// Hoodi (`560048`), the exit-gate chain: a fresh stack settles the committed PLONK proof against the live
/// `V6_1_0_SP1_VERIFIER_PLONK`. Runs when `HOODI_RPC_URL` is set.
contract HoodiForkTest is TestnetForkTest {
    uint256 internal constant HOODI_CHAIN_ID = 560048;
    uint256 internal constant HOODI_FORK_BLOCK = 3_919_000;

    function setUp() public override {
        _selectFork("HOODI_RPC_URL", HOODI_FORK_BLOCK, HOODI_CHAIN_ID);
    }

    function testForkSettlesTinyLlamaPlonkProof() public {
        registry = SashModelRegistry(deployCode("SashModelRegistry.sol:SashModelRegistry", abi.encode(address(this))));
        registry.enrollModel(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            TinyLlamaFixture.EOS_TOKEN_ID,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        Sp1SashSegmentVerifier adapter = _deploySp1Adapter(SP1_VERIFIER_PLONK, programVKey);
        VerifiableInferenceSettlement prodSettlement = _deploySettlement(address(adapter), false);
        assertFalse(prodSettlement.ALLOW_DEV_VERIFIER());

        vm.recordLogs();
        bytes32 runId = _settle(prodSettlement);
        assertEq(runId, SPEC_RUN_ID);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        uint256 settled;
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].topics[0] != VerifiableInferenceSettlement.InferenceVerified.selector) continue;
            settled++;
            assertEq(logs[i].emitter, address(prodSettlement));
            assertEq(logs[i].topics[1], SPEC_RUN_ID);
            (,, SashTypes.VerifierKind kind) = abi.decode(logs[i].data, (bytes32, bytes32, SashTypes.VerifierKind));
            assertEq(uint8(kind), uint8(SashTypes.VerifierKind.ZKVM_PLONK));
        }
        assertEq(settled, 1);
        assertTrue(prodSettlement.settledRuns(SPEC_RUN_ID));

        vm.expectRevert(abi.encodeWithSelector(SashTypes.RunAlreadySettled.selector, SPEC_RUN_ID));
        _settle(prodSettlement);
    }

    function _settle(VerifiableInferenceSettlement target) internal returns (bytes32) {
        return target.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proofBytes
        );
    }
}

/// Sepolia (`11155111`): control 30 (`SPEC.md` §2.1) against the live `V6_1_0_BROKEN_SP1_VERIFIER_PLONK`, whose
/// `VK_ROOT` differs from the pinned one. Runs when `SEPOLIA_RPC_URL` is set. The adapter is deployed with `new`,
/// so that its expected constructor revert stays inside the test.
contract SepoliaForkTest is TestnetForkTest {
    uint256 internal constant SEPOLIA_CHAIN_ID = 11155111;
    uint256 internal constant SEPOLIA_FORK_BLOCK = 12_029_000;

    function setUp() public override {
        _selectFork("SEPOLIA_RPC_URL", SEPOLIA_FORK_BLOCK, SEPOLIA_CHAIN_ID);
    }

    function testForkNegative30_LiveBrokenVkRootPlonkVerifierRejectedAtDeploy() public {
        assertEq(_deployment("V6_1_0_BROKEN_SP1_VERIFIER_PLONK"), BROKEN_SP1_VERIFIER_PLONK);
        assertEq(BROKEN_SP1_VERIFIER_PLONK.codehash, BROKEN_SP1_VERIFIER_PLONK_CODEHASH);
        SP1VerifierPlonk broken = SP1VerifierPlonk(BROKEN_SP1_VERIFIER_PLONK);
        assertEq(broken.VERSION(), "v6.1.0");
        assertEq(broken.VERIFIER_HASH(), PLONK_VERIFIER_HASH);
        assertEq(broken.VK_ROOT(), BROKEN_VK_ROOT);
        vm.expectRevert(
            abi.encodeWithSelector(
                Sp1SashSegmentVerifier.WrongSp1Verifier.selector, PLONK_VERIFIER_HASH, BROKEN_VK_ROOT
            )
        );
        new Sp1SashSegmentVerifier(BROKEN_SP1_VERIFIER_PLONK, programVKey);
    }
}
