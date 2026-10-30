// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

/// Test-only verifier whose `VERIFIER_HASH()` and `VK_ROOT()` return the constructor's arguments, for the
/// adapter's deployment check (`SECURITY.md` §6); it does not inherit `ISp1PlonkVerifierV6`, whose getters are
/// `pure`. Control 30 gives it the getters of Sepolia's
/// `V6_1_0_BROKEN_SP1_VERIFIER_PLONK` (`0xFaa0Ef4fC2d30bca021A982214E375cdeCFcc8f8`, sp1-contracts
/// `contracts/deployments/11155111.json`): the pinned `VERIFIER_HASH` and a stale `VK_ROOT`.
contract GetterStubPlonkVerifier {
    error Unreachable();

    bytes32 internal immutable verifierHash;
    bytes32 internal immutable vkRoot;

    constructor(bytes32 verifierHash_, bytes32 vkRoot_) {
        verifierHash = verifierHash_;
        vkRoot = vkRoot_;
    }

    // forge-lint: disable-next-line(mixed-case-function)
    function VERIFIER_HASH() external view returns (bytes32) {
        return verifierHash;
    }

    // forge-lint: disable-next-line(mixed-case-function)
    function VK_ROOT() external view returns (bytes32) {
        return vkRoot;
    }

    function verifyProof(bytes32, bytes calldata, bytes calldata) external pure {
        revert Unreachable();
    }
}
