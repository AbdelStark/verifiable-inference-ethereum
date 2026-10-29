// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

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
