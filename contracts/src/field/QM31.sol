// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "./M31.sol";
import {CM31} from "./CM31.sol";

/// @title QM31
/// @notice Quartic extension `QM31 = CM31[u] / (u^2 - (2 + i))` (`SecureField`) and SASH Horner evaluators.
library QM31 {
    struct Elem {
        uint32 a0;
        uint32 a1;
        uint32 b0;
        uint32 b1;
    }

    function zero() internal pure returns (Elem memory) {
        return Elem(0, 0, 0, 0);
    }

    function one() internal pure returns (Elem memory) {
        return Elem(1, 0, 0, 0);
    }

    function fromM31(uint32 x) internal pure returns (Elem memory) {
        return Elem(x, 0, 0, 0);
    }

    function checkCanonical(Elem memory x) internal pure {
        M31.checkCanonical(x.a0);
        M31.checkCanonical(x.a1);
        M31.checkCanonical(x.b0);
        M31.checkCanonical(x.b1);
    }

    function eq(Elem memory x, Elem memory y) internal pure returns (bool) {
        return x.a0 == y.a0 && x.a1 == y.a1 && x.b0 == y.b0 && x.b1 == y.b1;
    }

    function add(Elem memory x, Elem memory y) internal pure returns (Elem memory) {
        return Elem(
            M31.add(x.a0, y.a0),
            M31.add(x.a1, y.a1),
            M31.add(x.b0, y.b0),
            M31.add(x.b1, y.b1)
        );
    }

    function sub(Elem memory x, Elem memory y) internal pure returns (Elem memory) {
        return Elem(
            M31.sub(x.a0, y.a0),
            M31.sub(x.a1, y.a1),
            M31.sub(x.b0, y.b0),
            M31.sub(x.b1, y.b1)
        );
    }

    function mul(Elem memory x, Elem memory y) internal pure returns (Elem memory) {
        CM31.Elem memory xa = CM31.Elem(x.a0, x.a1);
        CM31.Elem memory xb = CM31.Elem(x.b0, x.b1);
        CM31.Elem memory ya = CM31.Elem(y.a0, y.a1);
        CM31.Elem memory yb = CM31.Elem(y.b0, y.b1);

        CM31.Elem memory aa = CM31.mul(xa, ya);
        CM31.Elem memory bb = CM31.mul(xb, yb);
        CM31.Elem memory realPart = CM31.add(aa, CM31.mulByR(bb));
        CM31.Elem memory uPart = CM31.add(CM31.mul(xa, yb), CM31.mul(xb, ya));
        return Elem(realPart.a0, realPart.a1, uPart.a0, uPart.a1);
    }

    function inv(Elem memory x) internal pure returns (Elem memory) {
        CM31.Elem memory xa = CM31.Elem(x.a0, x.a1);
        CM31.Elem memory xb = CM31.Elem(x.b0, x.b1);
        CM31.Elem memory denom = CM31.sub(CM31.mul(xa, xa), CM31.mulByR(CM31.mul(xb, xb)));
        CM31.Elem memory denomInv = CM31.inv(denom);
        CM31.Elem memory ra = CM31.mul(xa, denomInv);
        CM31.Elem memory rb = CM31.mul(CM31.neg(xb), denomInv);
        return Elem(ra.a0, ra.a1, rb.a0, rb.a1);
    }

    /// @notice SASH ascending Horner image `sum_{k=0..S-1} ids[k] * w^(S-1-k)` (`embed_tok` on `B0L0`).
    function hornerAsc(uint32[] calldata ids, Elem memory w) internal pure returns (Elem memory acc) {
        acc = zero();
        uint256 len = ids.length;
        for (uint256 i = 0; i < len; ++i) {
            uint32 tok = ids[i];
            M31.checkCanonical(tok);
            acc = add(mul(acc, w), fromM31(tok));
        }
    }

    /// @notice SASH descending Horner image `sum_{k=0..S-1} ids[k] * w^k` (`head_tok` on `head`).
    function hornerDesc(uint32[] calldata ids, Elem memory w) internal pure returns (Elem memory acc) {
        acc = zero();
        uint256 i = ids.length;
        while (i > 0) {
            unchecked {
                --i;
            }
            uint32 tok = ids[i];
            M31.checkCanonical(tok);
            acc = add(mul(acc, w), fromM31(tok));
        }
    }

    function writeLE(bytes memory buf, uint256 offset, Elem memory q) internal pure {
        writeU32LE(buf, offset, q.a0);
        writeU32LE(buf, offset + 4, q.a1);
        writeU32LE(buf, offset + 8, q.b0);
        writeU32LE(buf, offset + 12, q.b1);
    }

    function writeU32LE(bytes memory buf, uint256 offset, uint32 v) internal pure {
        buf[offset] = bytes1(uint8(v));
        buf[offset + 1] = bytes1(uint8(v >> 8));
        buf[offset + 2] = bytes1(uint8(v >> 16));
        buf[offset + 3] = bytes1(uint8(v >> 24));
    }
}
