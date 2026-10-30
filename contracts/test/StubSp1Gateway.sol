// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {ISp1PlonkVerifierV6} from "../src/sash/Sp1SashSegmentVerifier.sol";

/// A keccak tag check standing in for the SP1 verifier until the real PLONK proof settles. Its getters return
/// the pinned v6.1.0 `SP1VerifierPlonk`'s, so that `Sp1SashSegmentVerifier`'s constructor accepts it.
contract StubSp1Gateway is ISp1PlonkVerifierV6 {
    bytes32 public immutable EXPECTED_VKEY;
    error BadSp1Proof();

    constructor(bytes32 vkey) {
        EXPECTED_VKEY = vkey;
    }

    // forge-lint: disable-next-line(mixed-case-function)
    function VERIFIER_HASH() external pure override returns (bytes32) {
        return 0x5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac;
    }

    // forge-lint: disable-next-line(mixed-case-function)
    function VK_ROOT() external pure override returns (bytes32) {
        return 0x002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352;
    }

    function verifyProof(bytes32 programVKey, bytes calldata publicValues, bytes calldata proofBytes)
        external
        view
        override
    {
        bytes32 expectedProof = keccak256(abi.encodePacked(EXPECTED_VKEY, publicValues));
        if (programVKey != EXPECTED_VKEY || proofBytes.length != 32 || bytes32(proofBytes) != expectedProof) {
            revert BadSp1Proof();
        }
    }
}
