//! Mersenne-31 field tower (`M31`, `CM31`, `QM31`) matching Stwo / `NitrooZK-stwo`.

use core::ops::{Add, Mul, Neg, Sub};
use serde::{Deserialize, Serialize};

/// Mersenne-31 prime modulus `P = 2^31 - 1`.
pub const P: u32 = 0x7fff_ffff;
/// Twice the prime modulus `2P = 2^32 - 2`.
pub const TWO_P: u32 = 0xffff_fffe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonCanonicalM31(pub u32);

/// Canonical element of `GF(2^31 - 1)` in `[0, P - 1]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct M31(pub u32);

impl M31 {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);

    /// Validates canonical representation `< P`.
    #[inline]
    pub const fn new_checked(v: u32) -> Result<Self, NonCanonicalM31> {
        if v < P {
            Ok(Self(v))
        } else {
            Err(NonCanonicalM31(v))
        }
    }

    /// Exact reduction of `val` in `[0, P^2)` modulo `P = 2^31 - 1` (matches Stwo `M31::reduce`).
    #[inline]
    pub const fn reduce_u64(val: u64) -> Self {
        Self((((((val >> 31) + val + 1) >> 31) + val) & (P as u64)) as u32)
    }

    #[inline]
    pub const fn reduce_u32(val: u32) -> Self {
        Self::reduce_u64(val as u64)
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn square(self) -> Self {
        Self::reduce_u64((self.0 as u64) * (self.0 as u64))
    }

    pub const fn pow(self, mut exp: u32) -> Self {
        let mut base = self;
        let mut acc = Self::ONE;
        while exp > 0 {
            if (exp & 1) == 1 {
                acc = Self::reduce_u64((acc.0 as u64) * (base.0 as u64));
            }
            base = base.square();
            exp >>= 1;
        }
        acc
    }

    /// Multiplicative inverse `x^(P - 2) mod P`. Panics on `0`.
    pub fn inverse(self) -> Self {
        assert!(!self.is_zero(), "M31::0 has no multiplicative inverse");
        self.pow(P - 2)
    }
}

impl Add for M31 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        let s = self.0 + rhs.0;
        Self(s.checked_sub(P).unwrap_or(s))
    }
}

impl Sub for M31 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        if self.0 >= rhs.0 {
            Self(self.0 - rhs.0)
        } else {
            Self(self.0 + P - rhs.0)
        }
    }
}

impl Neg for M31 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        if self.0 == 0 {
            Self::ZERO
        } else {
            Self(P - self.0)
        }
    }
}

impl Mul for M31 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self::reduce_u64((self.0 as u64) * (rhs.0 as u64))
    }
}

/// Complex quadratic extension `CM31 = M31[i] / (i^2 + 1)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CM31(pub M31, pub M31);

impl CM31 {
    pub const ZERO: Self = Self(M31::ZERO, M31::ZERO);
    pub const ONE: Self = Self(M31::ONE, M31::ZERO);
    /// Irreducible constant `R = 2 + i` used to construct `QM31 = CM31[u]/(u^2 - R)`.
    pub const R: Self = Self(M31(2), M31(1));

    #[inline]
    pub fn is_zero(self) -> bool {
        self.0.is_zero() && self.1.is_zero()
    }

    /// Multiplies `self` by `R = 2 + i`: `(2*a - b) + (a + 2*b)i`.
    #[inline]
    pub fn mul_by_r(self) -> Self {
        let two_a = self.0 + self.0;
        let two_b = self.1 + self.1;
        Self(two_a - self.1, self.0 + two_b)
    }

    #[inline]
    pub fn conj(self) -> Self {
        Self(self.0, -self.1)
    }

    pub fn inverse(self) -> Self {
        assert!(!self.is_zero(), "CM31::0 has no multiplicative inverse");
        let norm_inv = (self.0.square() + self.1.square()).inverse();
        Self(self.0 * norm_inv, (-self.1) * norm_inv)
    }
}

impl Add for CM31 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Sub for CM31 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0, self.1 - rhs.1)
    }
}

impl Neg for CM31 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0, -self.1)
    }
}

impl Mul for CM31 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self(
            self.0 * rhs.0 - self.1 * rhs.1,
            self.0 * rhs.1 + self.1 * rhs.0,
        )
    }
}

/// Quartic secure extension `QM31 = CM31[u] / (u^2 - (2 + i))` (`SecureField`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QM31(pub CM31, pub CM31);

impl QM31 {
    pub const ZERO: Self = Self(CM31::ZERO, CM31::ZERO);
    pub const ONE: Self = Self(CM31::ONE, CM31::ZERO);

    #[inline]
    pub const fn from_u32_unchecked(a0: u32, a1: u32, b0: u32, b1: u32) -> Self {
        Self(CM31(M31(a0), M31(a1)), CM31(M31(b0), M31(b1)))
    }

    pub fn from_u32_checked(a0: u32, a1: u32, b0: u32, b1: u32) -> Result<Self, NonCanonicalM31> {
        Ok(Self(
            CM31(M31::new_checked(a0)?, M31::new_checked(a1)?),
            CM31(M31::new_checked(b0)?, M31::new_checked(b1)?),
        ))
    }

    #[inline]
    pub const fn from_m31(x: M31) -> Self {
        Self(CM31(x, M31::ZERO), CM31::ZERO)
    }

    #[inline]
    pub const fn to_m31_array(self) -> [M31; 4] {
        [self.0 .0, self.0 .1, self.1 .0, self.1 .1]
    }

    #[inline]
    pub const fn to_u32_array(self) -> [u32; 4] {
        [self.0 .0 .0, self.0 .1 .0, self.1 .0 .0, self.1 .1 .0]
    }

    #[inline]
    pub fn to_le_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (i, limb) in self.to_u32_array().into_iter().enumerate() {
            out[4 * i..4 * (i + 1)].copy_from_slice(&limb.to_le_bytes());
        }
        out
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.0.is_zero() && self.1.is_zero()
    }

    pub fn inverse(self) -> Self {
        assert!(!self.is_zero(), "QM31::0 has no multiplicative inverse");
        let b2 = self.1 * self.1;
        let denom_inv = (self.0 * self.0 - b2.mul_by_r()).inverse();
        Self(self.0 * denom_inv, (-self.1) * denom_inv)
    }

    /// Evaluates SASH's ascending Horner image `sum_{k=0..S-1} ids[k] * w^(S-1-k)` (`embed_tok`).
    pub fn horner_asc(ids: &[u32], w: Self) -> Result<Self, NonCanonicalM31> {
        let mut acc = Self::ZERO;
        for &v in ids {
            let m = M31::new_checked(v)?;
            acc = acc * w + Self::from_m31(m);
        }
        Ok(acc)
    }

    /// Evaluates SASH's descending Horner image `sum_{k=0..S-1} ids[k] * w^k` (`head_tok`).
    pub fn horner_desc(ids: &[u32], w: Self) -> Result<Self, NonCanonicalM31> {
        let mut acc = Self::ZERO;
        for &v in ids.iter().rev() {
            let m = M31::new_checked(v)?;
            acc = acc * w + Self::from_m31(m);
        }
        Ok(acc)
    }
}

impl Add for QM31 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Sub for QM31 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0, self.1 - rhs.1)
    }
}

impl Neg for QM31 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0, -self.1)
    }
}

impl Mul for QM31 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        let aa = self.0 * rhs.0;
        let bb = self.1 * rhs.1;
        Self(aa + bb.mul_by_r(), self.0 * rhs.1 + self.1 * rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m31_edge_reductions_and_inverses() {
        assert_eq!(M31::reduce_u32(0), M31(0));
        assert_eq!(M31::reduce_u32(P), M31(0));
        assert_eq!(M31::reduce_u32(P + 1), M31(1));
        assert_eq!(M31::reduce_u32(TWO_P), M31(0));
        assert_eq!(M31::reduce_u32(u32::MAX), M31(1));
        assert!(M31::new_checked(P).is_err());
        assert!(M31::new_checked(P - 1).is_ok());

        for x in [1u32, 2, 19, 123_456_789, P - 1] {
            let a = M31(x);
            assert_eq!(a * a.inverse(), M31::ONE);
        }
    }

    #[test]
    fn qm31_ring_and_inverse_laws() {
        let x = QM31::from_u32_unchecked(1259081484, 630063634, 1062868201, 1393098785);
        let y = QM31::from_u32_unchecked(1297757949, 994229370, 1478950871, 2099916732);
        let z = QM31::from_u32_unchecked(7, 13, 29, 997);

        assert_eq!((x + y) * z, x * z + y * z);
        assert_eq!((x * y) * z, x * (y * z));
        assert_eq!(x * x.inverse(), QM31::ONE);
        assert_eq!(y * y.inverse(), QM31::ONE);
    }
}
