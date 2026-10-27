//! Reference SASH 23-segment grid checks and the sha256 settlement digests (`SPEC.md` §1).

use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentClaim {
    pub segment_index: u16,
    pub pre_root: [u8; 32],
    pub model_root: [u8; 32],
    pub residual_in_root: [u8; 32],
    pub residual_out_root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelManifest {
    pub num_segments: u16,
    pub seq_len: u16,
    pub eos_token_id: u32,
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

    pub fn from_segments(segments: &[SegmentClaim], seq_len: u16, eos_token_id: u32) -> Self {
        Self {
            num_segments: segments.len() as u16,
            seq_len,
            eos_token_id,
            manifest_roots_hash: Self::compute_roots_hash(segments),
        }
    }
}

/// `sha256` over token ids, each encoded as a 32-byte big-endian word
/// (Solidity `sha256(abi.encodePacked(uint32[]))`).
pub fn compute_token_hash(ids: &[u32]) -> [u8; 32] {
    let mut h = Sha256::new();
    for id in ids {
        h.update([0u8; 28]);
        h.update(id.to_be_bytes());
    }
    h.finalize().into()
}

/// `sha256(modelId || be32(n) || n * (be16(segmentIndex) || preRoot || modelRoot || residualInRoot
/// || residualOutRoot) || tokenHash(tokIn) || tokenHash(tokOut))`
/// (matches Solidity `SashGridVerifier.computePublicDigest`).
pub fn compute_public_digest(
    model_id: [u8; 32],
    segments: &[SegmentClaim],
    tok_in: &[u32],
    tok_out: &[u32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(model_id);
    h.update((segments.len() as u32).to_be_bytes());
    for s in segments {
        h.update(s.segment_index.to_be_bytes());
        h.update(s.pre_root);
        h.update(s.model_root);
        h.update(s.residual_in_root);
        h.update(s.residual_out_root);
    }
    h.update(compute_token_hash(tok_in));
    h.update(compute_token_hash(tok_out));
    h.finalize().into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridError {
    InvalidSegmentCount { got: usize, expected: usize },
    InvalidSegmentIndex { position: usize, got: u16 },
    ManifestRootMismatch,
    ResidualBoundaryMismatch { link: usize },
    HeadMustNotCarryOutRoot,
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

    let s = manifest.seq_len as usize;
    if tok_in.len() != s || tok_out.len() != s {
        return Err(GridError::InvalidSequenceLength);
    }
    let p_len = prompt_ids.len();
    let a_len = answer_ids.len();
    if p_len == 0 || a_len == 0 || p_len + a_len - 1 != s {
        return Err(GridError::InvalidTokenWindow);
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tinyllama_fixture::*;

    #[test]
    fn real_tinyllama_23_segment_grid_accepts() {
        let segs = tinyllama_segments();
        let manifest = tinyllama_manifest(&segs);
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

    // TinyLlama values from `SPEC.md` §1.4.
    #[test]
    fn tinyllama_digests_match_spec() {
        let segs = tinyllama_segments();
        let manifest = tinyllama_manifest(&segs);
        assert_eq!(
            hex::encode(manifest.manifest_roots_hash),
            "fd8ecd8c79795b3a79e426d60a912d0a8a160188539c9b12dd3803b249c55b26"
        );
        assert_eq!(
            hex::encode(compute_token_hash(&TINYLLAMA_TOK_IN)),
            "3e683ee8603b85a4f5407e1af5f20a437ddadc9c098acc6aac357d533d3f32cb"
        );
        assert_eq!(
            hex::encode(compute_token_hash(&TINYLLAMA_TOK_OUT)),
            "44c6f339d896d66ea25d0ca6e15214f596ef5c701db79be94232be51d29b6b1e"
        );
        assert_eq!(
            hex::encode(compute_public_digest(
                tinyllama_model_id(),
                &segs,
                &TINYLLAMA_TOK_IN,
                &TINYLLAMA_TOK_OUT
            )),
            "8c27cadf1bc21512bbb76eb63b23699b9af32d18b92d7cdf5a90a489b8000c6b"
        );
        assert_eq!(
            hex::encode(compute_token_hash(&TINYLLAMA_PROMPT_IDS)),
            "b4d5730f21772191057fb4600dc73dfdf0350fab6996c7b12d1d1ca7b83b9ec1"
        );
        assert_eq!(
            hex::encode(compute_token_hash(&TINYLLAMA_ANSWER_IDS)),
            "aed36ba9347820959ae88d81e8c45a733e404b199fb3c12a463e2301253934c8"
        );
    }

    /// Runs the grid on the TinyLlama fixture with `segs`, `tok_in`, `prompt_ids` or `answer_ids`
    /// replaced where given (negative controls, `SPEC.md` §2.1).
    fn check(
        manifest: Option<ModelManifest>,
        segs: Option<&[SegmentClaim]>,
        tok_in: Option<&[u32]>,
        prompt_ids: Option<&[u32]>,
        answer_ids: Option<&[u32]>,
    ) -> Result<(), GridError> {
        let base = tinyllama_segments();
        let manifest = manifest.unwrap_or_else(|| tinyllama_manifest(&base));
        verify_sash_grid(
            &manifest,
            segs.unwrap_or(&base),
            tok_in.unwrap_or(&TINYLLAMA_TOK_IN),
            &TINYLLAMA_TOK_OUT,
            prompt_ids.unwrap_or(&TINYLLAMA_PROMPT_IDS),
            answer_ids.unwrap_or(&TINYLLAMA_ANSWER_IDS),
        )
    }

    #[test]
    fn control_01_forged_model_root() {
        let mut s = tinyllama_segments();
        s[7].model_root[31] ^= 0x01;
        assert_eq!(
            check(None, Some(&s), None, None, None),
            Err(GridError::ManifestRootMismatch)
        );
    }

    #[test]
    fn control_02_forged_pre_root() {
        let mut s = tinyllama_segments();
        s[0].pre_root[31] ^= 0x01;
        assert_eq!(
            check(None, Some(&s), None, None, None),
            Err(GridError::ManifestRootMismatch)
        );
    }

    #[test]
    fn control_03_out_of_order_segment_index() {
        let mut s = tinyllama_segments();
        s[5].segment_index = 6;
        assert_eq!(
            check(None, Some(&s), None, None, None),
            Err(GridError::InvalidSegmentIndex {
                position: 5,
                got: 6
            })
        );
    }

    #[test]
    fn control_04_broken_residual_boundary_link() {
        let mut s = tinyllama_segments();
        s[10].residual_out_root[31] ^= 0xff;
        assert_eq!(
            check(None, Some(&s), None, None, None),
            Err(GridError::ResidualBoundaryMismatch { link: 10 })
        );
    }

    #[test]
    fn control_05_head_non_zero_residual_out_root() {
        let mut s = tinyllama_segments();
        s[22].residual_out_root[31] = 1;
        assert_eq!(
            check(None, Some(&s), None, None, None),
            Err(GridError::HeadMustNotCarryOutRoot)
        );
    }

    #[test]
    fn control_10_prompt_mismatch() {
        let mut pr = TINYLLAMA_PROMPT_IDS;
        pr[4] = 9999;
        assert_eq!(
            check(None, None, None, Some(&pr), None),
            Err(GridError::PromptTokenMismatch { index: 4 })
        );
    }

    #[test]
    fn control_11_broken_autoregressive_loop() {
        let mut ti = TINYLLAMA_TOK_IN;
        ti[27] = 1111;
        assert_eq!(
            check(None, None, Some(&ti), None, None),
            Err(GridError::AutoregressiveLoopMismatch { step: 2 })
        );
    }

    #[test]
    fn control_12_missing_eos() {
        let mut m = tinyllama_manifest(&tinyllama_segments());
        m.eos_token_id = 999;
        assert_eq!(
            check(Some(m), None, None, None, None),
            Err(GridError::MissingEosTermination {
                got: 2,
                expected: 999
            })
        );
    }

    #[test]
    fn control_13_answer_mismatch() {
        let mut ans = TINYLLAMA_ANSWER_IDS;
        ans[3] = 777;
        assert_eq!(
            check(None, None, None, None, Some(&ans)),
            Err(GridError::AnswerTokenMismatch { index: 3 })
        );
    }

    #[test]
    fn control_16_truncated_segment_array() {
        let s = tinyllama_segments();
        assert_eq!(
            check(None, Some(&s[..22]), None, None, None),
            Err(GridError::InvalidSegmentCount {
                got: 22,
                expected: 23
            })
        );
    }
}
