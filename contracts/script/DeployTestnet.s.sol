// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Script} from "forge-std/Script.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// @title DeployTestnet
/// @notice Deploys the production-gated SASH EVM settlement stack (`allowDevVerifier = false`)
///         to Ethereum Sepolia (`11155111`) or Hoodi (`560048`) using `SP1_VERIFIER_GATEWAY` and
///         `SASH_AGGREGATOR_VKEY` from environment variables.
contract DeployTestnet is Script {
    function run()
        external
        returns (
            address registryAddr,
            address adapterAddr,
            address settlementAddr
        )
    {
        address sp1Gateway = vm.envAddress("SP1_VERIFIER_GATEWAY");
        bytes32 sashVKey = vm.envBytes32("SASH_AGGREGATOR_VKEY");

        vm.startBroadcast();

        SashModelRegistry registry = new SashModelRegistry(msg.sender);
        registry.enrollModel(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            TinyLlamaFixture.EOS_TOKEN_ID,
            TinyLlamaFixture.B0_REL_DRAWS,
            TinyLlamaFixture.HEAD_REL_DRAWS,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );

        Sp1SashSegmentVerifier adapter = new Sp1SashSegmentVerifier(sp1Gateway, sashVKey);
        VerifiableInferenceSettlement settlement =
            new VerifiableInferenceSettlement(address(registry), address(adapter), false);

        vm.stopBroadcast();
        return (address(registry), address(adapter), address(settlement));
    }
}
