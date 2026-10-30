// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Script} from "forge-std/Script.sol";
import {SP1Verifier as SP1VerifierPlonk} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {Sp1SashSegmentVerifier} from "../src/sash/Sp1SashSegmentVerifier.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// @title DeployAnvilSp1
/// @notice Deploys the production SASH EVM settlement stack on a local node (`chainid == 31337`): the v6.1.0
///         `SP1VerifierPlonk` compiled from the submodule, the registry with `TinyLlama-1.1B-Chat-v1.0`
///         (`y-attnout-w12`) enrolled, `Sp1SashSegmentVerifier` and `VerifiableInferenceSettlement` with
///         `allowDevVerifier = false`, and settles the committed PLONK proof
///         (`contracts/fixtures/tinyllama.plonk.json`). No dev verifier is deployed.
/// @dev The verifier is deployed with `vm.deployCode`, not `new`: its 8,992 B of initcode would take this script
///      over the 24,576 B `code_size_limit`. The broadcast JSON still names it (`contractName` `SP1Verifier`),
///      which `anvil-settle.sh` requires of every transaction.
contract DeployAnvilSp1 is Script {
    function run()
        external
        returns (
            address plonkVerifierAddr,
            address registryAddr,
            address adapterAddr,
            address settlementAddr,
            bytes32 settledRunId
        )
    {
        string memory json = vm.readFile(string.concat(vm.projectRoot(), "/fixtures/tinyllama.plonk.json"));
        bytes32 programVKey = vm.parseJsonBytes32(json, ".programVKey");
        bytes memory proofBytes = vm.parseJsonBytes(json, ".proofBytes");

        vm.startBroadcast();

        SP1VerifierPlonk plonkVerifier = SP1VerifierPlonk(vm.deployCode("SP1VerifierPlonk.sol:SP1Verifier"));
        SashModelRegistry registry = new SashModelRegistry(msg.sender);
        registry.enrollModel(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.NUM_SEGMENTS,
            TinyLlamaFixture.SEQ_LEN,
            TinyLlamaFixture.EOS_TOKEN_ID,
            TinyLlamaFixture.MANIFEST_ROOTS_HASH
        );
        Sp1SashSegmentVerifier adapter = new Sp1SashSegmentVerifier(address(plonkVerifier), programVKey);
        VerifiableInferenceSettlement settlement =
            new VerifiableInferenceSettlement(address(registry), address(adapter), false);

        settledRunId = settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            proofBytes
        );

        vm.stopBroadcast();
        return (address(plonkVerifier), address(registry), address(adapter), address(settlement), settledRunId);
    }
}
