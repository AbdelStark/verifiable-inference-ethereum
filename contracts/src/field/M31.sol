// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

/// @title M31
/// @notice Mersenne-31 base field arithmetic (`P = 2^31 - 1`) matching Stwo / `NitrooZK-stwo`.
library M31 {
    uint32 internal constant P = 0x7fffffff;
    uint32 internal constant TWO_P = 0xfffffffe;

    error NonCanonicalM31(uint32 value);
    error ZeroInverse();

    function checkCanonical(uint32 a) internal pure {
        // forge-lint: disable-next-line(require-revert-in-loop)
        if (a >= P) revert NonCanonicalM31(a);
    }

    /// @notice Exact reduction of `val` in `[0, P^2)` modulo `P = 2^31 - 1` (matches Stwo `M31::reduce`).
    function reduceU64(uint64 val) internal pure returns (uint32) {
        unchecked {
            // forge-lint: disable-next-line(unsafe-typecast)
            return uint32((((((val >> 31) + val + 1) >> 31) + val) & uint64(P)));
        }
    }

    function reduceU32(uint32 val) internal pure returns (uint32) {
        return reduceU64(uint64(val));
    }

    function add(uint32 a, uint32 b) internal pure returns (uint32) {
        unchecked {
            uint32 s = a + b;
            return s >= P ? s - P : s;
        }
    }

    function sub(uint32 a, uint32 b) internal pure returns (uint32) {
        unchecked {
            return a >= b ? a - b : (a + P) - b;
        }
    }

    function neg(uint32 a) internal pure returns (uint32) {
        return a == 0 ? 0 : P - a;
    }

    function mul(uint32 a, uint32 b) internal pure returns (uint32) {
        unchecked {
            return reduceU64(uint64(a) * uint64(b));
        }
    }

    function sq(uint32 a) internal pure returns (uint32) {
        return mul(a, a);
    }

    function pow(uint32 base, uint32 exp) internal pure returns (uint32) {
        uint32 acc = 1;
        uint32 b = base;
        uint32 e = exp;
        while (e > 0) {
            if ((e & 1) == 1) {
                acc = mul(acc, b);
            }
            b = sq(b);
            e >>= 1;
        }
        return acc;
    }

    function inv(uint32 a) internal pure returns (uint32) {
        if (a == 0) revert ZeroInverse();
        return pow(a, P - 2);
    }
}
