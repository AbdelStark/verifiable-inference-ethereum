// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "../field/M31.sol";
import {QM31} from "../field/QM31.sol";

/// @title CirclePoint
/// @notice Circle group `x^2 + y^2 = 1 mod P` over `M31` and `QM31`, subgroup generators, and Circle FRI folding.
library CirclePoint {
    uint32 internal constant GEN_X = 2;
    uint32 internal constant GEN_Y = 1268011823;

    error LogSizeTooLarge(uint32 logSize);

    struct PointM31 {
        uint32 x;
        uint32 y;
    }

    struct PointQM31 {
        QM31.Elem x;
        QM31.Elem y;
    }

    function zeroM31() internal pure returns (PointM31 memory) {
        return PointM31(1, 0);
    }

    function generatorM31() internal pure returns (PointM31 memory) {
        return PointM31(GEN_X, GEN_Y);
    }

    function isOnCurveM31(PointM31 memory p) internal pure returns (bool) {
        return M31.add(M31.sq(p.x), M31.sq(p.y)) == 1;
    }

    function addM31(PointM31 memory a, PointM31 memory b) internal pure returns (PointM31 memory) {
        return PointM31(
            M31.sub(M31.mul(a.x, b.x), M31.mul(a.y, b.y)),
            M31.add(M31.mul(a.x, b.y), M31.mul(a.y, b.x))
        );
    }

    function doubleM31(PointM31 memory a) internal pure returns (PointM31 memory) {
        return addM31(a, a);
    }

    /// @notice Circle FRI x-coordinate doubling map `pi(x) = 2 * x^2 - 1`.
    function doubleX(uint32 x) internal pure returns (uint32) {
        uint32 xx = M31.sq(x);
        return M31.sub(M31.add(xx, xx), 1);
    }

    function subgroupGen(uint32 logSize) internal pure returns (PointM31 memory p) {
        if (logSize > 31) revert LogSizeTooLarge(logSize);
        p = generatorM31();
        uint32 doublings = 31 - logSize;
        for (uint32 i = 0; i < doublings; ++i) {
            p = doubleM31(p);
        }
    }

    /// @notice Circle FRI univariate line fold step given evaluations `(f0, f1)` at `(x, -x)` (or `(p, -p)` with `invDenom`) and challenge `alpha`:
    /// `f_fold = (f0 + f1)/2 + alpha * (f0 - f1)/(2 * denom)`.
    function foldPairQM31(
        QM31.Elem memory f0,
        QM31.Elem memory f1,
        uint32 twoDenomInv,
        uint32 halfM31,
        QM31.Elem memory alpha
    ) internal pure returns (QM31.Elem memory) {
        QM31.Elem memory evenPart = QM31.mul(QM31.add(f0, f1), QM31.fromM31(halfM31));
        QM31.Elem memory oddSlope = QM31.mul(QM31.sub(f0, f1), QM31.fromM31(twoDenomInv));
        return QM31.add(evenPart, QM31.mul(alpha, oddSlope));
    }
}
