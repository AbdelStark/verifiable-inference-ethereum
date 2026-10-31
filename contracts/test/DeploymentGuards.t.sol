// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {DeployTestnet, SP1_VERIFIER_PLONK, SP1_VERIFIER_PLONK_CODEHASH} from "../script/DeployTestnet.s.sol";
import {DevTranscriptSegmentVerifier} from "../src/sash/DevTranscriptSegmentVerifier.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";
import {TinyLlamaBase} from "./TinyLlamaBase.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// The registry's owner checks and the dev verifier's deployment chain guard (`AGENTS.md` §2.3).
contract DeploymentGuardsTest is TinyLlamaBase {
    function testRegistryRejectsZeroOwner() public {
        vm.expectRevert(SashTypes.ZeroOwner.selector);
        new SashModelRegistry(address(0));
    }

    /// Only the owner enrolls a manifest or changes whether one is active.
    function testRegistryRejectsNonOwner() public {
        address stranger = makeAddr("stranger");
        bytes32 otherModelId = keccak256("other-model");

        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.UnauthorizedOwner.selector, stranger));
        registry.enrollModel(otherModelId, 23, 32, 2, TinyLlamaFixture.MANIFEST_ROOTS_HASH);

        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.UnauthorizedOwner.selector, stranger));
        registry.setModelActive(TinyLlamaFixture.MODEL_ID, false);

        // Neither call changed anything: the owner still enrolls the other model, and TinyLlama stays active.
        assertTrue(registry.getManifest(TinyLlamaFixture.MODEL_ID).active);
        registry.enrollModel(otherModelId, 23, 32, 2, TinyLlamaFixture.MANIFEST_ROOTS_HASH);
        registry.setModelActive(TinyLlamaFixture.MODEL_ID, false);
        vm.expectRevert(abi.encodeWithSelector(SashTypes.ModelNotActive.selector, TinyLlamaFixture.MODEL_ID));
        registry.getManifest(TinyLlamaFixture.MODEL_ID);
    }

    /// The dev verifier cannot be deployed on any chain but `31337`: Hoodi, Sepolia or mainnet.
    function testDevVerifierConstructorRejectsNonDevChain() public {
        uint256[3] memory chainIds = [uint256(560048), 11155111, 1];
        for (uint256 i = 0; i < chainIds.length; i++) {
            vm.chainId(chainIds[i]);
            vm.expectRevert(abi.encodeWithSelector(SashTypes.DevChainOnly.selector, chainIds[i]));
            new DevTranscriptSegmentVerifier();
        }
    }
}

/// `DeployTestnet`'s guards, each of which reverts before `vm.startBroadcast()` (`SettlementStack` holds the
/// post-deploy checks, which `HoodiForkTest` runs). The script is deployed with `deployCode`, so this file does not
/// embed its initcode.
contract DeployTestnetGuardsTest is Test {
    DeployTestnet internal script;

    function setUp() public {
        script = DeployTestnet(deployCode("DeployTestnet.s.sol:DeployTestnet"));
    }

    /// Only Hoodi and Sepolia: mainnet, where the same verifier is deployed, and the dev chain revert.
    function testRejectsOtherChains() public {
        uint256[2] memory chainIds = [uint256(1), 31337];
        for (uint256 i = 0; i < chainIds.length; i++) {
            vm.chainId(chainIds[i]);
            vm.expectRevert(abi.encodeWithSelector(DeployTestnet.UnsupportedChain.selector, chainIds[i]));
            script.run();
        }
    }

    /// Without a wallet or `--sender`, forge runs the script as `DEFAULT_SENDER`, which would become `OWNER`.
    function testRejectsDefaultSender() public {
        vm.chainId(560048);
        vm.prank(DEFAULT_SENDER);
        vm.expectRevert(DeployTestnet.DefaultSender.selector);
        script.run();
    }

    function testRejectsMissingSp1Verifier() public {
        vm.chainId(560048);
        assertEq(SP1_VERIFIER_PLONK.code.length, 0);
        vm.expectRevert(abi.encodeWithSelector(DeployTestnet.Sp1VerifierCodehashMismatch.selector, bytes32(0)));
        script.run();
    }

    /// The repository's compile of the v6.1.0 `SP1VerifierPlonk` returns the pinned getters, but its bytecode, and
    /// so its codehash, differ from the deployed one's (`SECURITY.md` §6).
    function testRejectsWrongSp1VerifierCode() public {
        vm.chainId(560048);
        bytes memory code = vm.getDeployedCode("SP1VerifierPlonk.sol:SP1Verifier");
        vm.etch(SP1_VERIFIER_PLONK, code);
        assertNotEq(keccak256(code), SP1_VERIFIER_PLONK_CODEHASH);
        vm.expectRevert(abi.encodeWithSelector(DeployTestnet.Sp1VerifierCodehashMismatch.selector, keccak256(code)));
        script.run();
    }
}
