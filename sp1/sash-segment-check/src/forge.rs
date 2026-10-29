//! G9's forgeries (`SPEC.md` §2.1), behind the `forge` feature: never in the guest. A published
//! token vector is changed at one position, with four slack positions solved so that its Horner
//! image under `w_residual_stream` stays the one the claims carry. `embed_tok_ok` and `head_tok_ok`
//! compare only that image, so a verifier whose transcript never absorbs the vectors accepts such a
//! forgery; SASH with `sp1/patches/0006` mixes them before `w` is drawn (`SPEC.md` §1.1), so a
//! changed vector draws another `w`.
//!
//! [`forge`] solves under the `w` the proof was made with, found by replaying both transcripts and
//! keeping the one whose `w` gives the claims' Horner image of the honest vector, and refuses a
//! segment where neither does. So a control built on it is the attack on that segment, never a
//! vector that would fail under every verifier.

use stwo::core::channel::{Blake2sM31Channel, Channel};
use stwo::core::fields::FieldExpOps;
use stwo::core::fields::m31::M31;
use stwo::core::fields::qm31::SecureField;
use stwo::core::pcs::CommitmentSchemeVerifier;
use stwo::core::vcs::blake2_merkle::Blake2sM31MerkleChannel;
use zk_prover::common::Proof;
use zk_prover::compose::{Claims, Fixed, Plan, Rels, unqm};
use zk_prover::util::pcs_config;

use crate::P;

/// The transcript the forger replays to `w_residual_stream` on the unmodified segment.
#[derive(Clone, Copy, Debug)]
pub enum Transcript {
    /// Upstream `eb95bfa`'s: trees 0 to 2, then the relation draws. The vectors never enter it.
    Upstream,
    /// `SPEC.md` §1.1's: the honest vectors' words mixed after tree 2, before the relation draws.
    TokensMixed,
}

/// Which published vector the forgery changes.
#[derive(Clone, Copy, Debug)]
pub enum Vector {
    /// `Fixed.tok_in`, which the embedding's Horner sum walks ascending.
    TokIn,
    /// `Fixed.tok_out`, which the argmax's Horner sum walks descending.
    TokOut,
}

/// `len(tok_in) || tok_in || len(tok_out) || tok_out`, the words `SPEC.md` §1.1 mixes, or `None`
/// when both vectors are empty. Written from the specification, not taken from the vendored tree.
pub fn spec_token_words(tok_in: &[u32], tok_out: &[u32]) -> Option<Vec<u32>> {
    if tok_in.is_empty() && tok_out.is_empty() {
        return None;
    }
    let len = |v: &[u32]| u32::try_from(v.len()).expect("a token vector fits u32");
    let mut words = vec![len(tok_in)];
    words.extend_from_slice(tok_in);
    words.push(len(tok_out));
    words.extend_from_slice(tok_out);
    Some(words)
}

/// `w_residual_stream` as `transcript` gives it on `fixed` and `proof`.
pub fn replayed_w(
    fixed: &Fixed,
    proof: &Proof,
    transcript: Transcript,
) -> Result<SecureField, String> {
    let plan = Plan::new(&fixed.params);
    if !plan.has_bindings() {
        return Err("the plan has no bindings, so no w_residual_stream".into());
    }
    if proof.commitments.len() < 3 || fixed.sizes.len() < 3 {
        return Err("fewer than three trees".into());
    }
    let config = pcs_config();
    let mut ch = Blake2sM31Channel::default();
    config.mix_into(&mut ch);
    let mut vcs = CommitmentSchemeVerifier::<Blake2sM31MerkleChannel>::new(config);
    for t in 0..3 {
        vcs.commit(proof.commitments[t], &fixed.sizes[t], &mut ch);
    }
    if let Transcript::TokensMixed = transcript
        && let Some(words) = spec_token_words(&fixed.tok_in, &fixed.tok_out)
    {
        ch.mix_u32s(&words);
    }
    Rels::draw(
        &mut ch,
        &plan.widths,
        plan.residual_stream.as_ref().map(|g| g.uns.as_slice()),
        plan.emb.map(|l| l.arity()),
        plan.needs_rq_rels(),
        plan.q_tabs.len(),
    );
    Ok(ch.draw_secure_felts(plan.n_challenges())[3])
}

fn m31(v: u32) -> M31 {
    M31::from_u32_unchecked(v % P)
}

/// The weight of position `i` of an `n`-id vector in its Horner sum: `w^(n-1-i)` ascending (the
/// embedding's), `w^i` descending (the argmax's).
fn weight(w: SecureField, n: usize, i: usize, ascending: bool) -> SecureField {
    let e = if ascending { n - 1 - i } else { i };
    w.pow(e as u128)
}

/// The Horner sum of `ids` under `w`, in `verify_composed`'s convention.
pub fn horner(ids: &[u32], w: SecureField, ascending: bool) -> SecureField {
    (0..ids.len()).fold(SecureField::default(), |acc, i| {
        acc + weight(w, ids.len(), i, ascending) * SecureField::from(m31(ids[i]))
    })
}

/// Solves `a x = b` over M31 for a 4x4 `a`, or `None` if `a` is singular: Gauss-Jordan on the
/// augmented rows.
fn solve(a: [[M31; 4]; 4], b: [M31; 4]) -> Option<[M31; 4]> {
    let zero = M31::from_u32_unchecked(0);
    let mut rows: [[M31; 5]; 4] =
        std::array::from_fn(|r| [a[r][0], a[r][1], a[r][2], a[r][3], b[r]]);
    for c in 0..4 {
        let pivot = (c..4).find(|&r| rows[r][c] != zero)?;
        rows.swap(c, pivot);
        let inv = rows[c][c].inverse();
        rows[c] = rows[c].map(|v| v * inv);
        let pivot_row = rows[c];
        for (r, row) in rows.iter_mut().enumerate() {
            let f = row[c];
            if r != c && f != zero {
                for (v, p) in row.iter_mut().zip(pivot_row) {
                    *v -= f * p;
                }
            }
        }
    }
    Some(rows.map(|row| row[4]))
}

/// `ids` with `ids[at] = value` and the four `slack` positions solved so that the Horner sum under
/// `w` is unchanged: one QM31 equation, four linear equations over M31. Every id stays `< P`.
pub fn horner_preserving(
    ids: &[u32],
    w: SecureField,
    ascending: bool,
    (at, value): (usize, u32),
    slack: [usize; 4],
) -> Result<Vec<u32>, String> {
    let n = ids.len();
    if at >= n || slack.iter().any(|&s| s >= n || s == at) || value >= P || ids[at] == value {
        return Err(format!(
            "bad forgery: position {at}, value {value}, slack {slack:?} over {n} ids"
        ));
    }
    let mut out = ids.to_vec();
    out[at] = value;
    let delta = weight(w, n, at, ascending) * SecureField::from(m31(value) - m31(ids[at]));
    let mut a = [[M31::from_u32_unchecked(0); 4]; 4];
    for (j, &s) in slack.iter().enumerate() {
        for (r, v) in weight(w, n, s, ascending)
            .to_m31_array()
            .into_iter()
            .enumerate()
        {
            a[r][j] = v;
        }
    }
    let d = solve(a, (-delta).to_m31_array()).ok_or("the slack weights are singular")?;
    for (j, &s) in slack.iter().enumerate() {
        out[s] = (m31(ids[s]) + d[j]).0;
    }
    if horner(&out, w, ascending) != horner(ids, w, ascending) || out.iter().any(|&v| v >= P) {
        return Err("the solved vector does not keep the Horner sum".into());
    }
    Ok(out)
}

impl Vector {
    /// The published vector's key in `fixed.json`.
    pub fn key(self) -> &'static str {
        match self {
            Vector::TokIn => "tok_in",
            Vector::TokOut => "tok_out",
        }
    }

    /// G9's edit: `tok_in[25]` (slack `1..=4`) or `tok_out[24]` (slack `0..=3`) set to 450.
    pub fn g9_edit(self) -> ((usize, u32), [usize; 4]) {
        match self {
            Vector::TokIn => ((25, 450), [1, 2, 3, 4]),
            Vector::TokOut => ((24, 450), [0, 1, 2, 3]),
        }
    }
}

/// A forged vector and the transcript whose `w` it keeps the Horner image under.
#[derive(Clone, Debug)]
pub struct Forgery {
    pub ids: Vec<u32>,
    pub transcript: Transcript,
}

/// G9's forgery of `vector` on the unmodified segment `fixed` / `claims` / `proof`: the edit of
/// [`Vector::g9_edit`], solved under the `w` the proof was made with. That `w` is the one, of the two
/// transcripts' (patch 0006's first), under which the honest vector's Horner sum is the claims'
/// `embed_tok` or `head_tok`; a segment where neither transcript gives it is refused.
pub fn forge(
    fixed: &Fixed,
    claims: &Claims,
    proof: &Proof,
    vector: Vector,
) -> Result<Forgery, String> {
    let (ids, claimed, ascending) = match vector {
        Vector::TokIn => (&fixed.tok_in, claims.embed_tok.first(), true),
        Vector::TokOut => (&fixed.tok_out, claims.head_tok.first(), false),
    };
    let key = vector.key();
    let claimed =
        unqm(*claimed.ok_or_else(|| format!("the claims carry no Horner image of {key}"))?);
    if ids.is_empty() {
        return Err(format!("the statement publishes no {key}"));
    }
    let (change, slack) = vector.g9_edit();
    for transcript in [Transcript::TokensMixed, Transcript::Upstream] {
        let w = replayed_w(fixed, proof, transcript)?;
        if horner(ids, w, ascending) == claimed {
            let ids = horner_preserving(ids, w, ascending, change, slack)?;
            return Ok(Forgery { ids, transcript });
        }
    }
    Err(format!(
        "neither transcript's w gives the claimed Horner image of the honest {key}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn felt(a: u32, b: u32, c: u32, d: u32) -> SecureField {
        SecureField::from_m31_array([a, b, c, d].map(M31::from_u32_unchecked))
    }

    /// Both conventions: the forged vector differs at the changed and the slack positions only,
    /// keeps every id `< P`, and has the honest Horner sum.
    #[test]
    fn the_forgery_keeps_the_horner_sum() {
        let w = felt(123_456_789, 987_654_321, 5, 2_000_000_011);
        let ids: Vec<u32> = (0..32).map(|i| 1000 + 37 * i).collect();
        for ascending in [true, false] {
            let forged = horner_preserving(&ids, w, ascending, (25, 450), [1, 2, 3, 4]).unwrap();
            assert_eq!(horner(&forged, w, ascending), horner(&ids, w, ascending));
            assert_eq!(forged[25], 450);
            for i in (0..32).filter(|i| ![1, 2, 3, 4, 25].contains(i)) {
                assert_eq!(forged[i], ids[i], "position {i}");
            }
            assert!(forged.iter().all(|&v| v < P));
            assert_ne!(forged, ids);
        }
    }

    #[test]
    fn a_forgery_that_changes_nothing_is_refused() {
        let w = felt(1, 2, 3, 4);
        let ids = [7u32; 8];
        assert!(horner_preserving(&ids, w, true, (5, 7), [0, 1, 2, 3]).is_err());
        assert!(horner_preserving(&ids, w, true, (1, 9), [0, 1, 2, 3]).is_err());
    }

    #[test]
    fn spec_words_prefix_each_vector_with_its_length() {
        assert_eq!(spec_token_words(&[], &[]), None);
        assert_eq!(spec_token_words(&[5, 6], &[7]), Some(vec![2, 5, 6, 1, 7]));
        assert_eq!(spec_token_words(&[], &[9]), Some(vec![0, 1, 9]));
    }
}
