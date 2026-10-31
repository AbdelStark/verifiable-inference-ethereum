// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Script} from "forge-std/Script.sol";
import {SP1Verifier as SP1VerifierPlonk} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {SettlementStack} from "./SettlementStack.sol";

/// @title DeployAnvilSp1
/// @notice Deploys the production SASH settlement stack on a local node (`chainid == 31337`) over the v6.1.0
///         `SP1VerifierPlonk` compiled from the submodule, and settles the committed PLONK proof
///         (`contracts/fixtures/tinyllama.plonk.json`) through `SettlementStack`, the code path `DeployTestnet`
///         broadcasts. No dev verifier is deployed.
/// @dev The verifier is deployed with `vm.deployCode`, not `new`: its 8,992 B of initcode would take this script
///      over the 24,576 B `code_size_limit`. The broadcast JSON still names it (`contractName` `SP1Verifier`),
///      which `anvil-settle.sh` requires of every transaction, as long as this file imports it: forge names a
///      broadcast contract only from the script's own sources.
contract DeployAnvilSp1 is Script, SettlementStack {
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
        (bytes32 programVKey, bytes memory proofBytes) = _readPlonkFixture();

        vm.startBroadcast();
        address plonkVerifier = address(SP1VerifierPlonk(vm.deployCode("SP1VerifierPlonk.sol:SP1Verifier")));
        Stack memory s = _deployAndSettle(msg.sender, plonkVerifier, programVKey, proofBytes);
        vm.stopBroadcast();

        _checkStack(s, msg.sender, plonkVerifier, programVKey);
        return (plonkVerifier, address(s.registry), address(s.adapter), address(s.settlement), s.runId);
    }
}
