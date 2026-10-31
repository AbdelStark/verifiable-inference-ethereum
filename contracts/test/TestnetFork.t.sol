// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {Vm} from "forge-std/Vm.sol";
import {SP1Verifier as SP1VerifierPlonk} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {BROKEN_VK_ROOT, PLONK_VERIFIER_HASH, VK_ROOT} from "./Sp1VerifierBinding.t.sol";
import {SPEC_RUN_ID} from "./TinyLlamaSettlement.t.sol";
import {SettlementStack} from "../script/SettlementStack.sol";
import {
    DeployTestnet,
    HOODI_CHAIN_ID,
    SEPOLIA_CHAIN_ID,
    SP1_VERIFIER_PLONK,
    SP1_VERIFIER_PLONK_CODEHASH
} from "../script/DeployTestnet.s.sol";

// Sepolia's `V6_1_0_BROKEN_SP1_VERIFIER_PLONK`: the pinned `VERSION` and `VERIFIER_HASH`, a stale `VK_ROOT`.
address constant BROKEN_SP1_VERIFIER_PLONK = 0xFaa0Ef4fC2d30bca021A982214E375cdeCFcc8f8;
bytes32 constant BROKEN_SP1_VERIFIER_PLONK_CODEHASH =
    0x413a62f179f67c0ca0ec1ee42b7bf4cee2cd1d68b420165ec673876658824465;

uint256 constant HOODI_FORK_BLOCK = 3_919_000;
uint256 constant SEPOLIA_FORK_BLOCK = 12_029_000;

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

    /// `DeployTestnet`'s pinned address and codehash against sp1-contracts' deployments file and the live chain.
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
/// `V6_1_0_SP1_VERIFIER_PLONK`, through `SettlementStack`, the code `DeployTestnet` broadcasts, and passes its
/// post-deploy checks. Runs when `HOODI_RPC_URL` is set. The script's own `run()` is never called here: its
/// `msg.sender` would be this contract while `vm.startBroadcast()` sends from `DEFAULT_SENDER`.
contract HoodiForkTest is TestnetForkTest, SettlementStack {
    function setUp() public override {
        _selectFork("HOODI_RPC_URL", HOODI_FORK_BLOCK, HOODI_CHAIN_ID);
    }

    function testForkSettlesTinyLlamaPlonkProof() public {
        vm.recordLogs();
        Stack memory s = _deployAndSettle(address(this), SP1_VERIFIER_PLONK, programVKey, proofBytes);
        _checkStack(s, address(this), SP1_VERIFIER_PLONK, programVKey);
        assertEq(s.runId, SPEC_RUN_ID);

        Vm.Log[] memory logs = vm.getRecordedLogs();
        uint256 settled;
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].topics[0] != VerifiableInferenceSettlement.InferenceVerified.selector) continue;
            settled++;
            assertEq(logs[i].emitter, address(s.settlement));
            assertEq(logs[i].topics[1], SPEC_RUN_ID);
            (,, SashTypes.VerifierKind kind) = abi.decode(logs[i].data, (bytes32, bytes32, SashTypes.VerifierKind));
            assertEq(uint8(kind), uint8(SashTypes.VerifierKind.ZKVM_PLONK));
        }
        assertEq(settled, 1);
        assertTrue(s.settlement.settledRuns(SPEC_RUN_ID));

        vm.expectRevert(abi.encodeWithSelector(SashTypes.RunAlreadySettled.selector, SPEC_RUN_ID));
        _settleTinyLlama(s.settlement, proofBytes);
    }
}

/// Sepolia (`11155111`): control 30 (`SPEC.md` §2.1) against the live `V6_1_0_BROKEN_SP1_VERIFIER_PLONK`, whose
/// `VK_ROOT` differs from the pinned one. Runs when `SEPOLIA_RPC_URL` is set. The adapter is deployed with `new`,
/// so that its expected constructor revert stays inside the test.
contract SepoliaForkTest is TestnetForkTest {
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

/// `DeployTestnet`'s RPC chain-id guard on a Hoodi fork: `block.chainid` overridden to Sepolia's, as `--chain
/// sepolia` or `FOUNDRY_CHAIN_ID` would, passes the chain-id and verifier guards (the pinned verifier is on Hoodi),
/// and the script then reverts on the RPC's own `eth_chainId` before `vm.startBroadcast()`. Runs when
/// `HOODI_RPC_URL` is set. The script is deployed with `deployCode`, so this file does not embed its initcode.
contract DeployTestnetForkGuardTest is Test {
    function testForkRejectsChainIdOverride() public {
        string memory url = vm.envOr("HOODI_RPC_URL", string(""));
        if (bytes(url).length == 0) {
            vm.skip(true);
            return;
        }
        vm.createSelectFork(url, HOODI_FORK_BLOCK);
        assertEq(block.chainid, HOODI_CHAIN_ID);
        DeployTestnet script = DeployTestnet(deployCode("DeployTestnet.s.sol:DeployTestnet"));

        vm.chainId(SEPOLIA_CHAIN_ID);
        vm.expectRevert(
            abi.encodeWithSelector(DeployTestnet.RpcChainIdMismatch.selector, HOODI_CHAIN_ID, SEPOLIA_CHAIN_ID)
        );
        script.run();
    }
}
