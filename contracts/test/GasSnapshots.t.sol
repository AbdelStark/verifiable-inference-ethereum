// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "../src/sash/SashTypes.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {SashGridHarness} from "./SashGridHarness.sol";

/// Gas snapshots of the TinyLlama run under Osaka, written to `contracts/snapshots/<group>.json`, which
/// `make check` and CI compare with the commit. Each budget is the first measurement plus 10%, rounded up
/// to the next 1,000; raise one only for an intended change.
contract GasSnapshotsTest is TinyLlamaBase {
    uint256 internal constant VERIFY_GRID_BUDGET = 41_000;
    uint256 internal constant COMPUTE_PUBLIC_DIGEST_BUDGET = 31_000;
    uint256 internal constant GET_MANIFEST_BUDGET = 8000;
    uint256 internal constant VERIFY_AND_SETTLE_BUDGET = 222_000;

    SashGridHarness internal harness;

    function setUp() public override {
        super.setUp();
        harness = SashGridHarness(deployCode("SashGridHarness.sol:SashGridHarness"));
    }

    function testGasVerifyGrid() public {
        SashTypes.ModelManifest memory manifest = registry.getManifest(TinyLlamaFixture.MODEL_ID);
        harness.verifyGrid(
            manifest,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds()
        );
        uint256 gasUsed = vm.snapshotGasLastFrame("SashGridVerifier", "verifyGrid");
        assertLe(gasUsed, VERIFY_GRID_BUDGET);
    }

    function testGasComputePublicDigest() public {
        bytes32 digest = harness.computePublicDigest(
            TinyLlamaFixture.MODEL_ID, TinyLlamaFixture.segments(), TinyLlamaFixture.tokIn(), TinyLlamaFixture.tokOut()
        );
        uint256 gasUsed = vm.snapshotGasLastFrame("SashGridVerifier", "computePublicDigest");
        assertEq(digest, TinyLlamaFixture.PUBLIC_DIGEST);
        assertLe(gasUsed, COMPUTE_PUBLIC_DIGEST_BUDGET);
    }

    function testGasGetManifest() public {
        registry.getManifest(TinyLlamaFixture.MODEL_ID);
        uint256 gasUsed = vm.snapshotGasLastFrame("SashModelRegistry", "getManifest");
        assertLe(gasUsed, GET_MANIFEST_BUDGET);
    }

    /// Under `isolate = true` the settlement call runs as its own transaction, so the frame includes the
    /// 21,000 intrinsic gas and the calldata cost (`EIP-7623` floor applied) and equals the anvil receipt.
    function testGasVerifyAndSettle() public {
        bytes memory proof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proof
        );
        uint256 gasUsed = vm.snapshotGasLastFrame("VerifiableInferenceSettlement", "verifyAndSettle");
        assertLe(gasUsed, VERIFY_AND_SETTLE_BUDGET);
    }
}
