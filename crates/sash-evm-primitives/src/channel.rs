//! Exact Stwo `Blake2sChannelGeneric<IS_M31_OUTPUT>` and SASH Fiat-Shamir replay.

use crate::blake2s::{blake2s_m31, blake2s256};
use crate::field::{M31, QM31, TWO_P};

pub type Blake2sChannel = Blake2sChannelGeneric<false>;
pub type Blake2sM31Channel = Blake2sChannelGeneric<true>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Blake2sChannelGeneric<const IS_M31_OUTPUT: bool> {
    digest: [u8; 32],
    n_draws: u32,
}

impl<const IS_M31_OUTPUT: bool> Blake2sChannelGeneric<IS_M31_OUTPUT> {
    pub const POW_PREFIX: u32 = 0x1234_5678;

    #[inline]
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    #[inline]
    pub const fn n_draws(&self) -> u32 {
        self.n_draws
    }

    #[inline]
    pub fn update_digest(&mut self, new_digest: [u8; 32]) {
        self.digest = new_digest;
        self.n_draws = 0;
    }

    #[inline]
    fn hash_bytes(data: &[u8]) -> [u8; 32] {
        if IS_M31_OUTPUT {
            blake2s_m31(data)
        } else {
            blake2s256(data)
        }
    }

    /// Matches `Blake2sMerkleChannelGeneric<IS_M31_OUTPUT>::mix_root`.
    pub fn mix_root(&mut self, root: [u8; 32]) {
        let mut msg = [0u8; 64];
        msg[..32].copy_from_slice(&self.digest);
        msg[32..].copy_from_slice(&root);
        self.update_digest(Self::hash_bytes(&msg));
    }

    /// Matches `Blake2sChannelGeneric<IS_M31_OUTPUT>::mix_felts`.
    pub fn mix_felts(&mut self, felts: &[QM31]) {
        let mut buf = Vec::with_capacity(32 + 16 * felts.len());
        buf.extend_from_slice(&self.digest);
        for q in felts {
            buf.extend_from_slice(&q.to_le_bytes());
        }
        self.update_digest(Self::hash_bytes(&buf));
    }

    pub fn draw_u32s(&mut self) -> [u32; 8] {
        let mut msg = [0u8; 37];
        msg[..32].copy_from_slice(&self.digest);
        msg[32..36].copy_from_slice(&self.n_draws.to_le_bytes());
        msg[36] = 0u8;
        self.n_draws += 1;

        let h = Self::hash_bytes(&msg);
        let mut out = [0u32; 8];
        for i in 0..8 {
            out[i] = u32::from_le_bytes([h[4 * i], h[4 * i + 1], h[4 * i + 2], h[4 * i + 3]]);
        }
        out
    }

    pub fn draw_base_felts(&mut self) -> [M31; 8] {
        loop {
            let words = self.draw_u32s();
            if words.iter().all(|&w| w < TWO_P) {
                return words.map(M31::reduce_u32);
            }
        }
    }

    pub fn draw_secure_felt(&mut self) -> QM31 {
        let f = self.draw_base_felts();
        QM31::from_u32_unchecked(f[0].0, f[1].0, f[2].0, f[3].0)
    }

    /// Matches Stwo `draw_secure_felts(n_felts)`: consumes `ceil(4 * n_felts / 8)` batches.
    pub fn draw_secure_felts(&mut self, n_felts: usize) -> Vec<QM31> {
        let batches = (4 * n_felts).div_ceil(8);
        let mut base = Vec::with_capacity(8 * batches);
        for _ in 0..batches {
            base.extend_from_slice(&self.draw_base_felts());
        }
        (0..n_felts)
            .map(|i| {
                QM31::from_u32_unchecked(
                    base[4 * i].0,
                    base[4 * i + 1].0,
                    base[4 * i + 2].0,
                    base[4 * i + 3].0,
                )
            })
            .collect()
    }
}

/// Derives SASH's `w_residual_stream` challenge (`f[3]`) from `(pre_root, model_root, trace_root)`
/// and `relation_draws` under SASH's fixed `(log_blowup=1, n_queries=100, pow_bits=0)` PCS config.
pub fn derive_sash_w_residual_stream(
    pre_root: [u8; 32],
    model_root: [u8; 32],
    trace_root: [u8; 32],
    relation_draws: u16,
) -> QM31 {
    let mut ch = Blake2sM31Channel::default();
    // PcsConfig::mix_into for (pow_bits=0, log_blowup=1, n_queries=100, log_last_layer=0,
    // line_fold_step=1, lifting_log_size=None->0)
    ch.mix_felts(&[
        QM31::from_u32_unchecked(0, 1, 100, 0),
        QM31::from_u32_unchecked(1, 0, 0, 0),
    ]);
    ch.mix_root(pre_root);
    ch.mix_root(model_root);
    ch.mix_root(trace_root);
    for _ in 0..relation_draws {
        let _ = ch.draw_secure_felts(2);
    }
    let f = ch.draw_secure_felts(5);
    f[3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stwo_channel_draw_counter_semantics() {
        let mut ch = Blake2sChannel::default();
        assert_eq!(ch.n_draws(), 0);
        let _ = ch.draw_u32s();
        assert_eq!(ch.n_draws(), 1);
        // 9 secure felts = 36 M31s -> ceil(36 / 8) = 5 batches -> 1 + 5 = 6
        let drawn = ch.draw_secure_felts(9);
        assert_eq!(drawn.len(), 9);
        assert_eq!(ch.n_draws(), 6);
    }

    // Same constants as `testTinyLlamaFiatShamirWResidualStreamParity` in Foundry.
    #[test]
    fn tinyllama_w_residual_stream_parity() {
        let segs = crate::tinyllama_fixture::tinyllama_segments();
        let w = |s: &crate::grid::SegmentClaim| {
            derive_sash_w_residual_stream(s.pre_root, s.model_root, s.trace_root, s.relation_draws)
                .to_u32_array()
        };
        assert_eq!(w(&segs[0]), [1259081484, 630063634, 1062868201, 1393098785]);
        assert_eq!(
            w(&segs[22]),
            [1297757949, 994229370, 1478950871, 2099916732]
        );
    }
}
