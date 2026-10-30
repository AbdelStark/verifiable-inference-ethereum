// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {ISp1PlonkVerifierV6} from "../src/sash/Sp1SashSegmentVerifier.sol";

/// Test-only stand-in for Sepolia's `V6_1_0_BROKEN_SP1_VERIFIER_PLONK` (`0xFaa0Ef4fC2d30bca021A982214E375cdeCFcc8f8`,
/// sp1-contracts `contracts/deployments/11155111.json`): the pinned `VERIFIER_HASH` and a stale `VK_ROOT`
/// (`SECURITY.md` §6).
contract BrokenVkRootPlonkVerifier is ISp1PlonkVerifierV6 {
    error Unreachable();

    // forge-lint: disable-next-line(mixed-case-function)
    function VERIFIER_HASH() external pure override returns (bytes32) {
        return 0x5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac;
    }

    // forge-lint: disable-next-line(mixed-case-function)
    function VK_ROOT() external pure override returns (bytes32) {
        return 0x00995aa7753b437dfe98b936e602e002155914af1922d96e1af288578adc2251;
    }

    function verifyProof(bytes32, bytes calldata, bytes calldata) external pure override {
        revert Unreachable();
    }
}
