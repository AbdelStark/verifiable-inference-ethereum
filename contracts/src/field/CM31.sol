// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "./M31.sol";

/// @title CM31
/// @notice Complex quadratic extension `CM31 = M31[i] / (i^2 + 1)`.
library CM31 {
    struct Elem {
        uint32 a0;
        uint32 a1;
    }

    function zero() internal pure returns (Elem memory) {
        return Elem(0, 0);
    }

    function one() internal pure returns (Elem memory) {
        return Elem(1, 0);
    }

    function eq(Elem memory x, Elem memory y) internal pure returns (bool) {
        return x.a0 == y.a0 && x.a1 == y.a1;
    }

    function add(Elem memory x, Elem memory y) internal pure returns (Elem memory) {
        return Elem(M31.add(x.a0, y.a0), M31.add(x.a1, y.a1));
    }

    function sub(Elem memory x, Elem memory y) internal pure returns (Elem memory) {
        return Elem(M31.sub(x.a0, y.a0), M31.sub(x.a1, y.a1));
    }

    function neg(Elem memory x) internal pure returns (Elem memory) {
        return Elem(M31.neg(x.a0), M31.neg(x.a1));
    }

    function mul(Elem memory x, Elem memory y) internal pure returns (Elem memory) {
        uint32 realPart = M31.sub(M31.mul(x.a0, y.a0), M31.mul(x.a1, y.a1));
        uint32 imagPart = M31.add(M31.mul(x.a0, y.a1), M31.mul(x.a1, y.a0));
        return Elem(realPart, imagPart);
    }

    /// @notice Multiplies `x` by `R = 2 + i`: `(2*a0 - a1) + (a0 + 2*a1)*i`.
    function mulByR(Elem memory x) internal pure returns (Elem memory) {
        uint32 twoA0 = M31.add(x.a0, x.a0);
        uint32 twoA1 = M31.add(x.a1, x.a1);
        return Elem(M31.sub(twoA0, x.a1), M31.add(x.a0, twoA1));
    }

    function inv(Elem memory x) internal pure returns (Elem memory) {
        uint32 normSq = M31.add(M31.sq(x.a0), M31.sq(x.a1));
        uint32 normInv = M31.inv(normSq);
        return Elem(M31.mul(x.a0, normInv), M31.mul(M31.neg(x.a1), normInv));
    }
}
