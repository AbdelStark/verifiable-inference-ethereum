//! Deterministic `SASHBIN1` binary segment repacker and validator.
//!
//! Converts SASH's verbose JSON segment files (`{tag}.fixed.json`, `{tag}.claims.json`,
//! `{tag}.proof.json`) into a flat, validated binary slice so a 32-bit RISC-V zkVM
//! guest can run `verify_composed` in `< 128 MiB` linear memory without JSON DOM bloat.

use sash_evm_primitives::field::{P, QM31};
use sash_evm_primitives::grid::SegmentClaim;

pub const SASHBIN_MAGIC: &[u8; 8] = b"SASHBIN1";
pub const EXPECTED_LOG_BLOWUP: u32 = 1;
pub const EXPECTED_N_QUERIES: u32 = 100;
pub const EXPECTED_POW_BITS: u32 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SashBinarySegment {
    pub claim: SegmentClaim,
    pub fixed_bytes: Vec<u8>,
    pub claims_no_gkr_bytes: Vec<u8>,
    pub gkr_bytes: Vec<u8>,
    pub stark_bin_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepackError {
    BadMagic,
    TruncatedBuffer,
    TrailingBytes,
    NonCanonicalM31(u32),
    SecurityParametersDrifted {
        log_blowup: u32,
        n_queries: u32,
        pow_bits: u32,
    },
    HeadOutRootNonZero,
}

impl SashBinarySegment {
    pub fn validate_header(
        log_blowup: u32,
        n_queries: u32,
        pow_bits: u32,
        is_head: bool,
        claim: &SegmentClaim,
    ) -> Result<(), RepackError> {
        if log_blowup != EXPECTED_LOG_BLOWUP
            || n_queries != EXPECTED_N_QUERIES
            || pow_bits != EXPECTED_POW_BITS
        {
            return Err(RepackError::SecurityParametersDrifted {
                log_blowup,
                n_queries,
                pow_bits,
            });
        }
        for limb in claim.token_horner_claim.to_u32_array() {
            if limb >= P {
                return Err(RepackError::NonCanonicalM31(limb));
            }
        }
        if is_head && claim.residual_out_root != [0u8; 32] {
            return Err(RepackError::HeadOutRootNonZero);
        }
        Ok(())
    }

    pub fn encode(&self) -> Vec<u8> {
        let total = 188
            + 4
            + self.fixed_bytes.len()
            + 4
            + self.claims_no_gkr_bytes.len()
            + 4
            + self.gkr_bytes.len()
            + 4
            + self.stark_bin_bytes.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(SASHBIN_MAGIC);
        out.extend_from_slice(&self.claim.segment_index.to_le_bytes());
        out.extend_from_slice(&self.claim.relation_draws.to_le_bytes());
        out.extend_from_slice(&self.claim.pre_root);
        out.extend_from_slice(&self.claim.model_root);
        out.extend_from_slice(&self.claim.trace_root);
        out.extend_from_slice(&self.claim.residual_in_root);
        out.extend_from_slice(&self.claim.residual_out_root);
        out.extend_from_slice(&self.claim.token_horner_claim.to_le_bytes());

        for slice in [
            &self.fixed_bytes,
            &self.claims_no_gkr_bytes,
            &self.gkr_bytes,
            &self.stark_bin_bytes,
        ] {
            out.extend_from_slice(&(slice.len() as u32).to_le_bytes());
            out.extend_from_slice(slice);
        }
        out
    }

    pub fn decode(buf: &[u8]) -> Result<Self, RepackError> {
        if buf.len() < 188 {
            return Err(RepackError::TruncatedBuffer);
        }
        if &buf[0..8] != SASHBIN_MAGIC {
            return Err(RepackError::BadMagic);
        }
        let segment_index = u16::from_le_bytes([buf[8], buf[9]]);
        let relation_draws = u16::from_le_bytes([buf[10], buf[11]]);

        let mut pre_root = [0u8; 32];
        pre_root.copy_from_slice(&buf[12..44]);
        let mut model_root = [0u8; 32];
        model_root.copy_from_slice(&buf[44..76]);
        let mut trace_root = [0u8; 32];
        trace_root.copy_from_slice(&buf[76..108]);
        let mut residual_in_root = [0u8; 32];
        residual_in_root.copy_from_slice(&buf[108..140]);
        let mut residual_out_root = [0u8; 32];
        residual_out_root.copy_from_slice(&buf[140..172]);

        let a0 = u32::from_le_bytes(buf[172..176].try_into().unwrap());
        let a1 = u32::from_le_bytes(buf[176..180].try_into().unwrap());
        let b0 = u32::from_le_bytes(buf[180..184].try_into().unwrap());
        let b1 = u32::from_le_bytes(buf[184..188].try_into().unwrap());
        let token_horner_claim = QM31::from_u32_checked(a0, a1, b0, b1)
            .map_err(|e| RepackError::NonCanonicalM31(e.0))?;

        let mut cursor = 188usize;
        let read_slice = |cursor: &mut usize| -> Result<Vec<u8>, RepackError> {
            if buf.len() < *cursor + 4 {
                return Err(RepackError::TruncatedBuffer);
            }
            let len = u32::from_le_bytes(buf[*cursor..*cursor + 4].try_into().unwrap()) as usize;
            *cursor += 4;
            if buf.len() < *cursor + len {
                return Err(RepackError::TruncatedBuffer);
            }
            let s = buf[*cursor..*cursor + len].to_vec();
            *cursor += len;
            Ok(s)
        };

        let fixed_bytes = read_slice(&mut cursor)?;
        let claims_no_gkr_bytes = read_slice(&mut cursor)?;
        let gkr_bytes = read_slice(&mut cursor)?;
        let stark_bin_bytes = read_slice(&mut cursor)?;

        if cursor != buf.len() {
            return Err(RepackError::TrailingBytes);
        }

        Ok(Self {
            claim: SegmentClaim {
                segment_index,
                relation_draws,
                pre_root,
                model_root,
                trace_root,
                residual_in_root,
                residual_out_root,
                token_horner_claim,
            },
            fixed_bytes,
            claims_no_gkr_bytes,
            gkr_bytes,
            stark_bin_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sash_evm_primitives::tinyllama_fixture::tinyllama_segments;

    #[test]
    fn sashbin_roundtrip_and_tamper_rejection() {
        let seg0 = tinyllama_segments().into_iter().next().unwrap();
        SashBinarySegment::validate_header(1, 100, 0, false, &seg0).unwrap();
        assert_eq!(
            SashBinarySegment::validate_header(1, 50, 0, false, &seg0),
            Err(RepackError::SecurityParametersDrifted {
                log_blowup: 1,
                n_queries: 50,
                pow_bits: 0
            })
        );

        let bin_seg = SashBinarySegment {
            claim: seg0,
            fixed_bytes: b"{\"layer\":0}".to_vec(),
            claims_no_gkr_bytes: b"{}".to_vec(),
            gkr_bytes: vec![1, 2, 3, 4],
            stark_bin_bytes: vec![9, 8, 7, 6, 5],
        };
        let encoded = bin_seg.encode();
        let decoded = SashBinarySegment::decode(&encoded).unwrap();
        assert_eq!(decoded, bin_seg);

        // Bad magic rejected
        let mut bad_magic = encoded.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            SashBinarySegment::decode(&bad_magic),
            Err(RepackError::BadMagic)
        );

        // Non-canonical M31 rejected
        let mut bad_m31 = encoded.clone();
        bad_m31[172..176].copy_from_slice(&P.to_le_bytes());
        assert_eq!(
            SashBinarySegment::decode(&bad_m31),
            Err(RepackError::NonCanonicalM31(P))
        );

        // Trailing bytes rejected
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            SashBinarySegment::decode(&trailing),
            Err(RepackError::TrailingBytes)
        );
    }
}
