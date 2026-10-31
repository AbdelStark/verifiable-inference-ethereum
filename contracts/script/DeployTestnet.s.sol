// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Script} from "forge-std/Script.sol";
import {SP1Verifier as SP1VerifierPlonk} from "@sp1-contracts/v6.1.0/SP1VerifierPlonk.sol";
import {SettlementStack} from "./SettlementStack.sol";

// `V6_1_0_SP1_VERIFIER_PLONK` on Hoodi, Sepolia and mainnet: the same runtime code on all three (`SECURITY.md` §6).
address constant SP1_VERIFIER_PLONK = 0xc3c6dDDAc8829b233Dc6536Ec024775a57b0AF2A;
bytes32 constant SP1_VERIFIER_PLONK_CODEHASH = 0xa5c9448eec43538d11533b207300fd13277c14c0800d33549a9492e7e8bdf84b;
string constant SP1_VERIFIER_PLONK_VERSION = "v6.1.0";
uint256 constant HOODI_CHAIN_ID = 560048;
uint256 constant SEPOLIA_CHAIN_ID = 11155111;

/// @title DeployTestnet
/// @notice Deploys the production SASH settlement stack (`SettlementStack`, `allowDevVerifier = false`) on Hoodi
///         (`560048`) or Sepolia (`11155111`) over the pinned SP1 v6.1.0 `SP1VerifierPlonk`, and settles the
///         committed TinyLlama PLONK proof: five transactions, the registry, `enrollModel`, the adapter, the
///         settlement and `verifyAndSettle`. It reads no environment variable: the verifier is pinned here, and
///         the aggregator vkey and the proof come from `contracts/fixtures/tinyllama.plonk.json`.
/// @dev Before the broadcast it checks the chain id, a sender other than forge's `DEFAULT_SENDER`, and the
///      verifier's codehash and `VERSION`; after it, the stack's immutables, manifest and settled `runId`
///      (`_checkStack`), so a dry run runs every check before anything is signed. The deployer (`--sender`)
///      becomes the registry's immutable `OWNER`, which can enroll new `modelId`s and deactivate TinyLlama
///      (`SashModelRegistry`). A broadcast cut short is recovered with `forge script … --resume`, not a second
///      entry point.
contract DeployTestnet is Script, SettlementStack {
    error UnsupportedChain(uint256 chainId);
    error DefaultSender();
    error Sp1VerifierCodehashMismatch(bytes32 got);
    error Sp1VerifierVersionMismatch(string got);

    function run() external returns (address registry, address adapter, address settlement, bytes32 runId) {
        if (block.chainid != HOODI_CHAIN_ID && block.chainid != SEPOLIA_CHAIN_ID) {
            revert UnsupportedChain(block.chainid);
        }
        if (msg.sender == DEFAULT_SENDER) revert DefaultSender();
        if (SP1_VERIFIER_PLONK.codehash != SP1_VERIFIER_PLONK_CODEHASH) {
            revert Sp1VerifierCodehashMismatch(SP1_VERIFIER_PLONK.codehash);
        }
        string memory version = SP1VerifierPlonk(SP1_VERIFIER_PLONK).VERSION();
        if (keccak256(bytes(version)) != keccak256(bytes(SP1_VERIFIER_PLONK_VERSION))) {
            revert Sp1VerifierVersionMismatch(version);
        }
        (bytes32 programVKey, bytes memory proofBytes) = _readPlonkFixture();

        vm.startBroadcast();
        Stack memory s = _deployAndSettle(msg.sender, SP1_VERIFIER_PLONK, programVKey, proofBytes);
        vm.stopBroadcast();

        _checkStack(s, msg.sender, SP1_VERIFIER_PLONK, programVKey);
        return (address(s.registry), address(s.adapter), address(s.settlement), s.runId);
    }
}
