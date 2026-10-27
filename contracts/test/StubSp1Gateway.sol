// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

contract StubSp1Gateway is ISP1Verifier {
    bytes32 public immutable EXPECTED_VKEY;
    error BadSp1Proof();

    constructor(bytes32 vkey) {
        EXPECTED_VKEY = vkey;
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
