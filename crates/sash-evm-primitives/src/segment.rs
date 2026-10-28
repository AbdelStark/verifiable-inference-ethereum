//! The segment guest's public values (`SPEC.md` §1.1): the 226 bytes it commits with
//! `commit_slice`, shared by the guest, its host and the aggregator.

use crate::grid::SegmentClaim;

/// Length of the encoded segment public values.
pub const SEGMENT_PUBLIC_VALUES_LEN: usize = 226;

/// Length of the §1.3 segment claim, the first bytes of the public values.
pub const SEGMENT_CLAIM_LEN: usize = 130;

/// One accepted segment's public values. Every hash and root is 32 raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentPublicValues {
    /// `segmentIndex`, `preRoot`, `modelRoot`, `residualInRoot`, `residualOutRoot`: bytes `0..130`.
    pub claim: SegmentClaim,
    /// `sha256` of the parsed `Fixed` statement with `tok_in` and `tok_out` cleared.
    pub fixed_hash: [u8; 32],
    /// `tokenHash(tok_in)` on the segment with the embedding gadget, else zero.
    pub tok_in_hash: [u8; 32],
    /// `tokenHash(tok_out)` on the segment that composes the argmax, else zero.
    pub tok_out_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    InvalidLength { got: usize },
}

impl SegmentPublicValues {
    pub fn encode(&self) -> [u8; SEGMENT_PUBLIC_VALUES_LEN] {
        let c = &self.claim;
        let mut out = [0u8; SEGMENT_PUBLIC_VALUES_LEN];
        out[0..2].copy_from_slice(&c.segment_index.to_be_bytes());
        for (i, field) in [
            &c.pre_root,
            &c.model_root,
            &c.residual_in_root,
            &c.residual_out_root,
            &self.fixed_hash,
            &self.tok_in_hash,
            &self.tok_out_hash,
        ]
        .into_iter()
        .enumerate()
        {
            out[2 + 32 * i..34 + 32 * i].copy_from_slice(field);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let bytes: &[u8; SEGMENT_PUBLIC_VALUES_LEN] = bytes
            .try_into()
            .map_err(|_| DecodeError::InvalidLength { got: bytes.len() })?;
        let word = |i: usize| -> [u8; 32] {
            bytes[2 + 32 * i..34 + 32 * i]
                .try_into()
                .expect("a 32-byte slice")
        };
        Ok(Self {
            claim: SegmentClaim {
                segment_index: u16::from_be_bytes([bytes[0], bytes[1]]),
                pre_root: word(0),
                model_root: word(1),
                residual_in_root: word(2),
                residual_out_root: word(3),
            },
            fixed_hash: word(4),
            tok_in_hash: word(5),
            tok_out_hash: word(6),
        })
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::tinyllama_fixture::*;
    use crate::token::compute_token_hash;

    /// No `fixedHash` is in the fixture yet; a stand-in that differs from every other field.
    fn stand_in_fixed_hash(tag: &str) -> [u8; 32] {
        Sha256::digest(format!("fixed statement of {tag}")).into()
    }

    fn b0l0() -> SegmentPublicValues {
        SegmentPublicValues {
            claim: tinyllama_segments()[0].clone(),
            fixed_hash: stand_in_fixed_hash("B0L0"),
            tok_in_hash: compute_token_hash(&TINYLLAMA_TOK_IN),
            tok_out_hash: [0; 32],
        }
    }

    fn head() -> SegmentPublicValues {
        SegmentPublicValues {
            claim: tinyllama_segments()[22].clone(),
            fixed_hash: stand_in_fixed_hash("head"),
            tok_in_hash: [0; 32],
            tok_out_hash: compute_token_hash(&TINYLLAMA_TOK_OUT),
        }
    }

    /// The offsets of `SPEC.md` §1.1's table, field by field.
    fn assert_layout(v: &SegmentPublicValues, bytes: &[u8; SEGMENT_PUBLIC_VALUES_LEN]) {
        assert_eq!(bytes[0..2], v.claim.segment_index.to_be_bytes());
        assert_eq!(bytes[2..34], v.claim.pre_root);
        assert_eq!(bytes[34..66], v.claim.model_root);
        assert_eq!(bytes[66..98], v.claim.residual_in_root);
        assert_eq!(bytes[98..130], v.claim.residual_out_root);
        assert_eq!(bytes[130..162], v.fixed_hash);
        assert_eq!(bytes[162..194], v.tok_in_hash);
        assert_eq!(bytes[194..226], v.tok_out_hash);
    }

    #[test]
    fn b0l0_and_head_follow_the_spec_layout() {
        let (b0l0, head) = (b0l0(), head());
        let (e0, e22) = (b0l0.encode(), head.encode());
        assert_layout(&b0l0, &e0);
        assert_layout(&head, &e22);
        assert_eq!(e0[0..2], [0x00, 0x00]);
        assert_eq!(e22[0..2], [0x00, 0x16]);
        // `SPEC.md` §1.4's tokInHash and tokOutHash; the head commits no residual out side.
        assert_eq!(
            hex::encode(&e0[162..194]),
            "3e683ee8603b85a4f5407e1af5f20a437ddadc9c098acc6aac357d533d3f32cb"
        );
        assert_eq!(e0[194..226], [0; 32]);
        assert_eq!(e22[162..194], [0; 32]);
        assert_eq!(
            hex::encode(&e22[194..226]),
            "44c6f339d896d66ea25d0ca6e15214f596ef5c701db79be94232be51d29b6b1e"
        );
        assert_eq!(e22[98..130], [0; 32]);
        assert_eq!(SegmentPublicValues::decode(&e0), Ok(b0l0));
        assert_eq!(SegmentPublicValues::decode(&e22), Ok(head));
    }

    /// Bytes `0..130` are the §1.3 segment claim: the 23 TinyLlama claims cut from their public
    /// values give `SPEC.md` §1.4's `publicDigest`.
    #[test]
    fn claim_bytes_give_the_spec_public_digest() {
        let mut h = Sha256::new();
        h.update(tinyllama_model_id());
        h.update(23u32.to_be_bytes());
        for claim in tinyllama_segments() {
            let v = SegmentPublicValues {
                claim,
                fixed_hash: [0xff; 32],
                tok_in_hash: [0xff; 32],
                tok_out_hash: [0xff; 32],
            };
            h.update(&v.encode()[..SEGMENT_CLAIM_LEN]);
        }
        h.update(compute_token_hash(&TINYLLAMA_TOK_IN));
        h.update(compute_token_hash(&TINYLLAMA_TOK_OUT));
        assert_eq!(
            hex::encode(h.finalize()),
            "8c27cadf1bc21512bbb76eb63b23699b9af32d18b92d7cdf5a90a489b8000c6b"
        );
    }

    #[test]
    fn decode_rejects_any_other_length() {
        let e = b0l0().encode();
        for len in [0, 130, 225, 227, 256] {
            let mut bytes = e.to_vec();
            bytes.resize(len, 0);
            assert_eq!(
                SegmentPublicValues::decode(&bytes),
                Err(DecodeError::InvalidLength { got: len })
            );
        }
    }
}
