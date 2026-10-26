// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {Vm} from "../test/TestBase.sol";
import {DevTranscriptSegmentVerifier} from "../src/sash/DevTranscriptSegmentVerifier.sol";
import {SashModelRegistry} from "../src/sash/SashModelRegistry.sol";
import {VerifiableInferenceSettlement} from "../src/sash/VerifiableInferenceSettlement.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";

/// @title DeployDevAnvil
/// @notice Deploys the SASH EVM settlement stack in local dev mode (`chainid == 31337`), enrolls
///         `TinyLlama-1.1B-Chat-v1.0` (`y-attnout-w12`), and executes a complete 23-segment verification.
contract DeployDevAnvil {
    Vm private constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run()
        external
        returns (
            address registryAddr,
            address devVerifierAddr,
            address settlementAddr,
            bytes32 settledRunId
        )
    {
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

        DevTranscriptSegmentVerifier devVerifier = new DevTranscriptSegmentVerifier();
        VerifiableInferenceSettlement settlement =
            new VerifiableInferenceSettlement(address(registry), address(devVerifier), true);

        bytes memory devProof = devVerifier.computeDevProof(TinyLlamaFixture.PUBLIC_DIGEST);
        settledRunId = settlement.verifyAndSettle(
            TinyLlamaFixture.MODEL_ID,
            TinyLlamaFixture.segments(),
            TinyLlamaFixture.tokIn(),
            TinyLlamaFixture.tokOut(),
            TinyLlamaFixture.promptIds(),
            TinyLlamaFixture.answerIds(),
            devProof
        );

        vm.stopBroadcast();
        return (address(registry), address(devVerifier), address(settlement), settledRunId);
    }
}
