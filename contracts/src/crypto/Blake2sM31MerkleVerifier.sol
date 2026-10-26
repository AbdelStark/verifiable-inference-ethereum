// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "../field/M31.sol";
import {QM31} from "../field/QM31.sol";
import {Blake2s} from "./Blake2s.sol";

/// @title Blake2sM31MerkleVerifier
/// @notice Verifies Stwo `Blake2sMerkleHasherGeneric<true>` Merkle openings over `M31` leaf columns.
library Blake2sM31MerkleVerifier {
    function hashLeafM31(uint32[] calldata leafColumns) internal pure returns (bytes32) {
        uint256 n = leafColumns.length;
        bytes memory buf = new bytes(4 * n);
        for (uint256 i = 0; i < n; ++i) {
            uint32 v = leafColumns[i];
            M31.checkCanonical(v);
            QM31.writeU32LE(buf, 4 * i, v);
        }
        return Blake2s.hashM31(buf);
    }

    function verifyOpening(
        bytes32 root,
        uint256 index,
        uint32[] calldata leafColumns,
        bytes32[] calldata siblings
    ) internal pure returns (bool) {
        bytes32 cur = hashLeafM31(leafColumns);
        uint256 idx = index;
        uint256 depth = siblings.length;
        for (uint256 d = 0; d < depth; ++d) {
            bytes32 sib = siblings[d];
            if ((idx & 1) == 0) {
                cur = Blake2s.hashPairM31(cur, sib);
            } else {
                cur = Blake2s.hashPairM31(sib, cur);
            }
            idx >>= 1;
        }
        return cur == root;
    }
}
