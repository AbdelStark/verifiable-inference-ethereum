//! Reference SASH 23-segment grid, manifest, Fiat-Shamir channel, and Horner verifier.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::channel::derive_sash_w_residual_stream;
use crate::field::{P, QM31};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentClaim {
    pub segment_index: u16,
    pub relation_draws: u16,
    pub pre_root: [u8; 32],
    pub model_root: [u8; 32],
    pub trace_root: [u8; 32],
    pub residual_in_root: [u8; 32],
    pub residual_out_root: [u8; 32],
    pub token_horner_claim: QM31,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManifest {
    pub num_segments: u16,
    pub seq_len: u16,
    pub eos_token_id: u32,
    pub b0_rel_draws: u16,
    pub head_rel_draws: u16,
    pub manifest_roots_hash: [u8; 32],
}

impl ModelManifest {
    pub fn compute_roots_hash(segments: &[SegmentClaim]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for seg in segments {
            hasher.update(seg.pre_root);
            hasher.update(seg.model_root);
        }
        hasher.finalize().into()
    }

    pub fn from_segments(
        segments: &[SegmentClaim],
        seq_len: u16,
        eos_token_id: u32,
        b0_rel_draws: u16,
        head_rel_draws: u16,
    ) -> Self {
        Self {
            num_segments: segments.len() as u16,
            seq_len,
            eos_token_id,
            b0_rel_draws,
            head_rel_draws,
            manifest_roots_hash: Self::compute_roots_hash(segments),
        }
    }
}

/// Computes the canonical 32-byte `SHA-256` digest binding `model_id` and all `SegmentClaim`s
/// (matches Solidity `VerifiableInferenceSettlement.computePublicDigest`).
pub fn compute_public_digest(model_id: [u8; 32], segments: &[SegmentClaim]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(model_id);
    h.update((segments.len() as u32).to_be_bytes());
    for s in segments {
        h.update(s.segment_index.to_be_bytes());
        h.update(s.relation_draws.to_be_bytes());
        h.update(s.pre_root);
        h.update(s.model_root);
        h.update(s.trace_root);
        h.update(s.residual_in_root);
        h.update(s.residual_out_root);
        for limb in s.token_horner_claim.to_u32_array() {
            h.update(limb.to_be_bytes());
        }
    }
    h.finalize().into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridError {
    InvalidSegmentCount { got: usize, expected: usize },
    InvalidSegmentIndex { position: usize, got: u16 },
    ManifestRootMismatch,
    ResidualBoundaryMismatch { link: usize },
    HeadMustNotCarryOutRoot,
    RelationDrawCountMismatch { segment: usize },
    NonCanonicalM31(u32),
    EmbedTokenHornerMismatch,
    HeadTokenHornerMismatch,
    InvalidSequenceLength,
    InvalidTokenWindow,
    PromptTokenMismatch { index: usize },
    AutoregressiveLoopMismatch { step: usize },
    AnswerTokenMismatch { index: usize },
    MissingEosTermination { got: u32, expected: u32 },
}

pub fn verify_sash_grid(
    manifest: &ModelManifest,
    segments: &[SegmentClaim],
    tok_in: &[u32],
    tok_out: &[u32],
    prompt_ids: &[u32],
    answer_ids: &[u32],
) -> Result<(), GridError> {
    let expected_n = manifest.num_segments as usize;
    if expected_n < 2 || segments.len() != expected_n {
        return Err(GridError::InvalidSegmentCount {
            got: segments.len(),
            expected: expected_n,
        });
    }

    for (pos, seg) in segments.iter().enumerate() {
        if seg.segment_index as usize != pos {
            return Err(GridError::InvalidSegmentIndex {
                position: pos,
                got: seg.segment_index,
            });
        }
        for limb in seg.token_horner_claim.to_u32_array() {
            if limb >= P {
                return Err(GridError::NonCanonicalM31(limb));
            }
        }
    }

    if ModelManifest::compute_roots_hash(segments) != manifest.manifest_roots_hash {
        return Err(GridError::ManifestRootMismatch);
    }

    let last = expected_n - 1;
    for i in 0..last {
        if segments[i].residual_out_root != segments[i + 1].residual_in_root {
            return Err(GridError::ResidualBoundaryMismatch { link: i });
        }
    }
    if segments[last].residual_out_root != [0u8; 32] {
        return Err(GridError::HeadMustNotCarryOutRoot);
    }

    if segments[0].relation_draws != manifest.b0_rel_draws {
        return Err(GridError::RelationDrawCountMismatch { segment: 0 });
    }
    if segments[last].relation_draws != manifest.head_rel_draws {
        return Err(GridError::RelationDrawCountMismatch { segment: last });
    }

    let s = manifest.seq_len as usize;
    if tok_in.len() != s || tok_out.len() != s {
        return Err(GridError::InvalidSequenceLength);
    }
    let p_len = prompt_ids.len();
    let a_len = answer_ids.len();
    if p_len == 0 || a_len == 0 || p_len + a_len - 1 != s {
        return Err(GridError::InvalidTokenWindow);
    }

    for &t in tok_in
        .iter()
        .chain(tok_out.iter())
        .chain(prompt_ids)
        .chain(answer_ids)
    {
        if t >= P {
            return Err(GridError::NonCanonicalM31(t));
        }
    }

    for j in 0..p_len {
        if tok_in[j] != prompt_ids[j] {
            return Err(GridError::PromptTokenMismatch { index: j });
        }
    }
    for j in 0..(a_len - 1) {
        if tok_in[p_len + j] != tok_out[p_len - 1 + j] {
            return Err(GridError::AutoregressiveLoopMismatch { step: j });
        }
    }
    for j in 0..a_len {
        if tok_out[p_len - 1 + j] != answer_ids[j] {
            return Err(GridError::AnswerTokenMismatch { index: j });
        }
    }
    let final_tok = tok_out[p_len + a_len - 2];
    if final_tok != manifest.eos_token_id {
        return Err(GridError::MissingEosTermination {
            got: final_tok,
            expected: manifest.eos_token_id,
        });
    }

    // Replay Fiat-Shamir channel on segment 0 (B0L0) and verify ascending Horner(tok_in).
    let w_b0 = derive_sash_w_residual_stream(
        segments[0].pre_root,
        segments[0].model_root,
        segments[0].trace_root,
        segments[0].relation_draws,
    );
    let embed_horner =
        QM31::horner_asc(tok_in, w_b0).map_err(|e| GridError::NonCanonicalM31(e.0))?;
    if embed_horner != segments[0].token_horner_claim {
        return Err(GridError::EmbedTokenHornerMismatch);
    }

    // Replay Fiat-Shamir channel on final segment (head) and verify descending Horner(tok_out).
    let w_head = derive_sash_w_residual_stream(
        segments[last].pre_root,
        segments[last].model_root,
        segments[last].trace_root,
        segments[last].relation_draws,
    );
    let head_horner =
        QM31::horner_desc(tok_out, w_head).map_err(|e| GridError::NonCanonicalM31(e.0))?;
    if head_horner != segments[last].token_horner_claim {
        return Err(GridError::HeadTokenHornerMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tinyllama_fixture::*;

    #[test]
    fn real_tinyllama_23_segment_grid_accepts() {
        let manifest = tinyllama_manifest();
        let segs = tinyllama_segments();
        verify_sash_grid(
            &manifest,
            &segs,
            &TINYLLAMA_TOK_IN,
            &TINYLLAMA_TOK_OUT,
            &TINYLLAMA_PROMPT_IDS,
            &TINYLLAMA_ANSWER_IDS,
        )
        .expect("verified TinyLlama-1.1B 23-segment SASH proof must pass");
    }

    #[test]
    fn negative_controls_reject_all_tampered_bundles() {
        let manifest = tinyllama_manifest();
        let base_segs = tinyllama_segments();

        // 1. Forged model_root on layer 7
        {
            let mut s = base_segs.clone();
            s[7].model_root[0] ^= 0x01;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::ManifestRootMismatch)
            );
        }

        // 2. Forged pre_root on layer 0
        {
            let mut s = base_segs.clone();
            s[0].pre_root[31] ^= 0x80;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::ManifestRootMismatch)
            );
        }

        // 3. Out-of-order segment_index
        {
            let mut s = base_segs.clone();
            s[5].segment_index = 6;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::InvalidSegmentIndex {
                    position: 5,
                    got: 6
                })
            );
        }

        // 4. Broken residual-stream Merkle root link between B0L10 and B0L11
        {
            let mut s = base_segs.clone();
            s[10].residual_out_root[3] ^= 0xff;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::ResidualBoundaryMismatch { link: 10 })
            );
        }

        // 5. Non-zero residual_out_root on head
        {
            let mut s = base_segs.clone();
            s[22].residual_out_root[0] = 1;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::HeadMustNotCarryOutRoot)
            );
        }

        // 6. Mutated tok_in (with matching prompt_ids) caught by B0L0 Fiat-Shamir Horner check
        {
            let mut ti = TINYLLAMA_TOK_IN;
            let mut pr = TINYLLAMA_PROMPT_IDS;
            ti[0] = 530;
            pr[0] = 530;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs,
                    &ti,
                    &TINYLLAMA_TOK_OUT,
                    &pr,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::EmbedTokenHornerMismatch)
            );
        }

        // 7. Mutated tok_out (before prompt window) caught by head Fiat-Shamir Horner check
        {
            let mut to = TINYLLAMA_TOK_OUT;
            to[0] ^= 1;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs,
                    &TINYLLAMA_TOK_IN,
                    &to,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::HeadTokenHornerMismatch)
            );
        }

        // 8. Mutated trace_root on B0L0 shifts w_residual_stream and breaks embed_tok
        {
            let mut s = base_segs.clone();
            s[0].trace_root[0] ^= 0x01;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::EmbedTokenHornerMismatch)
            );
        }

        // 9. Wrong relation_draws on head
        {
            let mut s = base_segs.clone();
            s[22].relation_draws = 25;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::RelationDrawCountMismatch { segment: 22 })
            );
        }

        // 10. Public prompt_ids mismatch
        {
            let mut pr = TINYLLAMA_PROMPT_IDS;
            pr[10] = 9999;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &pr,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::PromptTokenMismatch { index: 10 })
            );
        }

        // 11. Broken autoregressive teacher-forcing loop
        {
            let mut ti = TINYLLAMA_TOK_IN;
            ti[27] = 1111;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs,
                    &ti,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::AutoregressiveLoopMismatch { step: 2 })
            );
        }

        // 12. Missing EOS termination
        {
            let mut m = manifest.clone();
            m.eos_token_id = 999;
            assert_eq!(
                verify_sash_grid(
                    &m,
                    &base_segs,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::MissingEosTermination {
                    got: 2,
                    expected: 999
                })
            );
        }

        // 13. Public answer_ids mismatch
        {
            let mut ans = TINYLLAMA_ANSWER_IDS;
            ans[5] = 42;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &ans
                ),
                Err(GridError::AnswerTokenMismatch { index: 5 })
            );
        }

        // 14. Non-canonical M31 limb in token_horner_claim
        {
            let mut s = base_segs.clone();
            s[1].token_horner_claim = QM31::from_u32_unchecked(P, 0, 0, 0);
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &s,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::NonCanonicalM31(P))
            );
        }

        // 15. Non-canonical token ID >= P
        {
            let mut pr = TINYLLAMA_PROMPT_IDS;
            pr[0] = P;
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs,
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &pr,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::NonCanonicalM31(P))
            );
        }

        // 16. Wrong segment count (truncated grid)
        {
            assert_eq!(
                verify_sash_grid(
                    &manifest,
                    &base_segs[..22],
                    &TINYLLAMA_TOK_IN,
                    &TINYLLAMA_TOK_OUT,
                    &TINYLLAMA_PROMPT_IDS,
                    &TINYLLAMA_ANSWER_IDS
                ),
                Err(GridError::InvalidSegmentCount {
                    got: 22,
                    expected: 23
                })
            );
        }
    }
}
