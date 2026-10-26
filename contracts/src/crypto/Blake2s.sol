// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {M31} from "../field/M31.sol";

/// @title Blake2s
/// @notice Unkeyed RFC 7693 `BLAKE2s-256` and Stwo `Blake2sM31` (`reduce_to_m31`) primitives.
library Blake2s {
    uint32 private constant IV0 = 0x6A09E667;
    uint32 private constant IV1 = 0xBB67AE85;
    uint32 private constant IV2 = 0x3C6EF372;
    uint32 private constant IV3 = 0xA54FF53A;
    uint32 private constant IV4 = 0x510E527F;
    uint32 private constant IV5 = 0x9B05688C;
    uint32 private constant IV6 = 0x1F83D9AB;
    uint32 private constant IV7 = 0x5BE0CD19;

    function rotr32(uint32 x, uint32 n) private pure returns (uint32) {
        unchecked {
            return (x >> n) | (x << (32 - n));
        }
    }

    function g(
        uint32[16] memory v,
        uint256 a,
        uint256 b,
        uint256 c,
        uint256 d,
        uint32 x,
        uint32 y
    ) private pure {
        unchecked {
            v[a] = v[a] + v[b] + x;
            v[d] = rotr32(v[d] ^ v[a], 16);
            v[c] = v[c] + v[d];
            v[b] = rotr32(v[b] ^ v[c], 12);
            v[a] = v[a] + v[b] + y;
            v[d] = rotr32(v[d] ^ v[a], 8);
            v[c] = v[c] + v[d];
            v[b] = rotr32(v[b] ^ v[c], 7);
        }
    }

    function round(uint32[16] memory v, uint32[16] memory m, uint64 s) private pure {
        g(v, 0, 4, 8, 12, m[s & 0xf], m[(s >> 4) & 0xf]);
        g(v, 1, 5, 9, 13, m[(s >> 8) & 0xf], m[(s >> 12) & 0xf]);
        g(v, 2, 6, 10, 14, m[(s >> 16) & 0xf], m[(s >> 20) & 0xf]);
        g(v, 3, 7, 11, 15, m[(s >> 24) & 0xf], m[(s >> 28) & 0xf]);
        g(v, 0, 5, 10, 15, m[(s >> 32) & 0xf], m[(s >> 36) & 0xf]);
        g(v, 1, 6, 11, 12, m[(s >> 40) & 0xf], m[(s >> 44) & 0xf]);
        g(v, 2, 7, 8, 13, m[(s >> 48) & 0xf], m[(s >> 52) & 0xf]);
        g(v, 3, 4, 9, 14, m[(s >> 56) & 0xf], m[(s >> 60) & 0xf]);
    }

    function compress(
        uint32[8] memory h,
        uint32[16] memory m,
        uint64 t,
        bool isLast
    ) private pure {
        uint32[16] memory v;
        for (uint256 i = 0; i < 8; ++i) {
            v[i] = h[i];
        }
        v[8] = IV0;
        v[9] = IV1;
        v[10] = IV2;
        v[11] = IV3;
        v[12] = IV4 ^ uint32(t);
        v[13] = IV5 ^ uint32(t >> 32);
        v[14] = isLast ? ~IV6 : IV6;
        v[15] = IV7;

        // Exact RFC 7693 SIGMA rows packed little-nibble-first (index k at bits 4*k..4*k+3):
        round(v, m, 0xfedcba9876543210); // [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        round(v, m, 0x357b20c16df984ae); // [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3]
        round(v, m, 0x491763eadf250c8b); // [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4]
        round(v, m, 0x8f04a562ebcd1397); // [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8]
        round(v, m, 0xd386cb1efa427509); // [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13]
        round(v, m, 0x91ef57d438b0a6c2); // [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9]
        round(v, m, 0xb8293670a4def15c); // [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11]
        round(v, m, 0xa2684f05931ce7bd); // [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10]
        round(v, m, 0x5a417d2c803b9ef6); // [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5]
        round(v, m, 0x0dc3e9bf5167482a); // [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0]

        for (uint256 i = 0; i < 8; ++i) {
            h[i] ^= v[i] ^ v[i + 8];
        }
    }

    function loadBlockWords(bytes memory data, uint256 offset, uint256 rem)
        private
        pure
        returns (uint32[16] memory m)
    {
        uint8[64] memory blk;
        for (uint256 i = 0; i < rem; ++i) {
            blk[i] = uint8(data[offset + i]);
        }
        for (uint256 w = 0; w < 16; ++w) {
            uint256 j = 4 * w;
            m[w] = uint32(blk[j])
                | (uint32(blk[j + 1]) << 8)
                | (uint32(blk[j + 2]) << 16)
                | (uint32(blk[j + 3]) << 24);
        }
    }

    /// @notice Unkeyed 32-byte RFC 7693 `BLAKE2s-256`.
    function hash(bytes memory data) internal pure returns (bytes32 out) {
        uint32[8] memory h = [
            IV0 ^ uint32(0x01010020),
            IV1,
            IV2,
            IV3,
            IV4,
            IV5,
            IV6,
            IV7
        ];

        uint256 len = data.length;
        if (len == 0) {
            uint32[16] memory emptyBlock;
            compress(h, emptyBlock, 0, true);
        } else {
            uint256 offset = 0;
            while (len - offset > 64) {
                uint32[16] memory m = loadBlockWords(data, offset, 64);
                offset += 64;
                compress(h, m, uint64(offset), false);
            }
            uint32[16] memory lastBlock = loadBlockWords(data, offset, len - offset);
            compress(h, lastBlock, uint64(len), true);
        }

        bytes memory buf = new bytes(32);
        for (uint256 i = 0; i < 8; ++i) {
            uint32 w = h[i];
            buf[4 * i] = bytes1(uint8(w));
            buf[4 * i + 1] = bytes1(uint8(w >> 8));
            buf[4 * i + 2] = bytes1(uint8(w >> 16));
            buf[4 * i + 3] = bytes1(uint8(w >> 24));
        }
        assembly ("memory-safe") {
            out := mload(add(buf, 32))
        }
    }

    /// @notice Reduces each of the 8 LE `uint32` words modulo `P = 2^31 - 1` (Stwo `reduce_to_m31`).
    function reduceToM31(bytes32 raw) internal pure returns (bytes32 out) {
        bytes memory buf = new bytes(32);
        for (uint256 i = 0; i < 8; ++i) {
            uint256 j = 4 * i;
            uint32 w = uint32(uint8(raw[j]))
                | (uint32(uint8(raw[j + 1])) << 8)
                | (uint32(uint8(raw[j + 2])) << 16)
                | (uint32(uint8(raw[j + 3])) << 24);
            uint32 r = M31.reduceU32(w);
            buf[j] = bytes1(uint8(r));
            buf[j + 1] = bytes1(uint8(r >> 8));
            buf[j + 2] = bytes1(uint8(r >> 16));
            buf[j + 3] = bytes1(uint8(r >> 24));
        }
        assembly ("memory-safe") {
            out := mload(add(buf, 32))
        }
    }

    /// @notice `Blake2sM31Hasher::hash(data)` = `reduceToM31(blake2s256(data))`.
    function hashM31(bytes memory data) internal pure returns (bytes32) {
        return reduceToM31(hash(data));
    }

    function hashPairM31(bytes32 left, bytes32 right) internal pure returns (bytes32) {
        return hashM31(abi.encodePacked(left, right));
    }

    function extractWordsLE(bytes32 digest) internal pure returns (uint32[8] memory words) {
        for (uint256 i = 0; i < 8; ++i) {
            uint256 j = 4 * i;
            words[i] = uint32(uint8(digest[j]))
                | (uint32(uint8(digest[j + 1])) << 8)
                | (uint32(uint8(digest[j + 2])) << 16)
                | (uint32(uint8(digest[j + 3])) << 24);
        }
    }
}
