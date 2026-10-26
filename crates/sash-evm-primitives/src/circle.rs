//! Circle curve (`x^2 + y^2 = 1 mod P`) and `Blake2sM31` Merkle opening verifier.

use crate::blake2s::hash_merkle_node_m31;
use crate::field::{M31, QM31};

/// Canonical order-`2^31` generator of the Mersenne-31 circle group (`stwo/src/core/circle.rs`).
pub const M31_CIRCLE_GEN: CirclePointM31 = CirclePointM31 {
    x: M31(2),
    y: M31(1_268_011_823),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CirclePointM31 {
    pub x: M31,
    pub y: M31,
}

impl CirclePointM31 {
    pub const ZERO: Self = Self {
        x: M31::ONE,
        y: M31::ZERO,
    };

    #[inline]
    pub fn is_on_curve(self) -> bool {
        self.x.square() + self.y.square() == M31::ONE
    }

    #[inline]
    pub fn add_point(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x - self.y * rhs.y,
            y: self.x * rhs.y + self.y * rhs.x,
        }
    }

    #[inline]
    pub fn double(self) -> Self {
        self.add_point(self)
    }

    /// Circle FRI x-coordinate folding map `pi(x) = 2*x^2 - 1`.
    #[inline]
    pub fn double_x(x: M31) -> M31 {
        let xx = x.square();
        xx + xx - M31::ONE
    }

    /// Subgroup generator of order `2^log_size` (`log_size <= 31`).
    pub fn subgroup_gen(log_size: u32) -> Self {
        assert!(
            log_size <= 31,
            "log_size exceeds M31 circle group 2-adicity"
        );
        let mut p = M31_CIRCLE_GEN;
        for _ in 0..(31 - log_size) {
            p = p.double();
        }
        p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CirclePointQM31 {
    pub x: QM31,
    pub y: QM31,
}

impl CirclePointQM31 {
    #[inline]
    pub fn is_on_curve(self) -> bool {
        self.x * self.x + self.y * self.y == QM31::ONE
    }

    #[inline]
    pub fn add_point(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x - self.y * rhs.y,
            y: self.x * rhs.y + self.y * rhs.x,
        }
    }
}

/// Single-path `Blake2sM31` Merkle decommitment verifier for a uniform-height tree.
pub fn verify_merkle_opening_m31(
    root: [u8; 32],
    mut index: u32,
    leaf_columns: &[M31],
    siblings: &[[u8; 32]],
) -> bool {
    let mut cur = hash_merkle_node_m31(None, leaf_columns);
    for &sib in siblings {
        let children = if (index & 1) == 0 {
            (cur, sib)
        } else {
            (sib, cur)
        };
        cur = hash_merkle_node_m31(Some(children), &[]);
        index >>= 1;
    }
    cur == root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_generator_order_and_curve_invariant() {
        assert!(M31_CIRCLE_GEN.is_on_curve());
        let g4 = CirclePointM31::subgroup_gen(4);
        assert!(g4.is_on_curve());
        let mut p = g4;
        for _ in 0..4 {
            p = p.double();
        }
        assert_eq!(p, CirclePointM31::ZERO);
    }

    #[test]
    fn merkle_opening_accepts_valid_and_rejects_tampered_sibling() {
        let leaf0 = [M31(10), M31(20)];
        let leaf1 = [M31(30), M31(40)];
        let leaf2 = [M31(50), M31(60)];
        let leaf3 = [M31(70), M31(80)];

        let h0 = hash_merkle_node_m31(None, &leaf0);
        let h1 = hash_merkle_node_m31(None, &leaf1);
        let h2 = hash_merkle_node_m31(None, &leaf2);
        let h3 = hash_merkle_node_m31(None, &leaf3);
        let h01 = hash_merkle_node_m31(Some((h0, h1)), &[]);
        let h23 = hash_merkle_node_m31(Some((h2, h3)), &[]);
        let root = hash_merkle_node_m31(Some((h01, h23)), &[]);

        assert!(verify_merkle_opening_m31(root, 2, &leaf2, &[h3, h01]));
        assert!(!verify_merkle_opening_m31(root, 1, &leaf2, &[h3, h01]));
        let mut bad = h3;
        bad[0] ^= 1;
        assert!(!verify_merkle_opening_m31(root, 2, &leaf2, &[bad, h01]));
    }
}
