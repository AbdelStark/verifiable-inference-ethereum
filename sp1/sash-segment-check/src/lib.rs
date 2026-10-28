//! The segment guest's check (`SPEC.md` §1.1) as a plain library, so that every rule runs under
//! `cargo test` natively and the guest only reads its three buffers, calls [`check_segment`] and
//! commits the encoded [`SegmentPublicValues`].
//!
//! It runs SASH's own `verify_composed` (the no-`prover` build of `sp1/vendor/sash`) and the rules
//! upstream does not enforce, in `SPEC.md` §1.1's order:
//! - `Malformed`: `fixed` does not parse;
//! - 1 to 5 on the parsed `Fixed` and its `Plan`, before any verification: `TokenIdOutOfField`,
//!   `SecurityParams`, `MissingLayer`, `UnboundEdge`, `UnsoundDequant`;
//! - 6 and 7 on the `Verdict`: `NotAccepted`, `OpenSeam`.
//!
//! A `Plan::new` assertion or any other panic in the verifier build is a rejection with no
//! variant: natively the check panics, in the guest it exits 1.

use std::fmt;

use sash_evm_primitives::grid::{SegmentClaim, compute_token_hash};
pub use sash_evm_primitives::segment::SegmentPublicValues;
use serde_json::Value;
use sha2::{Digest, Sha256};
use zk_prover::bf16::KMAX_SOUND;
use zk_prover::boundary::Axis;
use zk_prover::common::Proof;
use zk_prover::compose::{Claims, Edge, EdgeState, Fixed, Plan, Verdict, verify_composed_parsed};
use zk_prover::gadgets::dequant::Spec;

/// The M31 modulus `P = 2^31 - 1`. SASH maps token ids into M31 unreduced
/// (`M31::from_u32_unchecked`), so an id `P + k` would alias `k`.
pub const P: u32 = (1 << 31) - 1;

/// The security parameters `SPEC.md` §1.1 rule 2 admits.
pub const LOG_BLOWUP: u32 = 1;
pub const N_QUERIES: usize = 100;
pub const POW_BITS: u32 = 0;

/// Why a segment is rejected, one variant per `SPEC.md` §1.1 rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    /// `fixed` is not a `Fixed` statement.
    Malformed(String),
    /// Rule 1: a token id is not below `P`.
    TokenIdOutOfField {
        vector: &'static str,
        index: usize,
        id: u32,
    },
    /// Rule 2.
    SecurityParams {
        log_blowup: u32,
        n_queries: usize,
        pow_bits: u32,
    },
    /// Rule 3: `layer` is absent or not below `2^16`.
    MissingLayer { layer: Option<usize> },
    /// Rule 4: the binding table's unbound edges, and whether the argmax is in its lax form.
    UnboundEdge { edges: Vec<u8>, argmax_lax: bool },
    /// Rule 5: `kmax`, and the indices of the matmuls whose dequant `Spec` is not sound.
    UnsoundDequant { kmax: u32, mats: Vec<usize> },
    /// Rule 6: the `Verdict`'s failed checks and error, or why `claims` or `proof` did not decode.
    NotAccepted {
        failed: Vec<String>,
        error: Option<String>,
    },
    /// Rule 7: the open seams upstream's `accepts()` ignores.
    OpenSeam {
        deq_out_open: Vec<String>,
        rq_scale_open: Vec<String>,
        ao_q_open: bool,
        nn_open: bool,
    },
}

impl Reject {
    pub fn name(&self) -> &'static str {
        match self {
            Reject::Malformed(_) => "Malformed",
            Reject::TokenIdOutOfField { .. } => "TokenIdOutOfField",
            Reject::SecurityParams { .. } => "SecurityParams",
            Reject::MissingLayer { .. } => "MissingLayer",
            Reject::UnboundEdge { .. } => "UnboundEdge",
            Reject::UnsoundDequant { .. } => "UnsoundDequant",
            Reject::NotAccepted { .. } => "NotAccepted",
            Reject::OpenSeam { .. } => "OpenSeam",
        }
    }
}

impl fmt::Display for Reject {
    /// The variant's name first, so the guest's panic message names it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {self:?}", self.name())
    }
}

/// Checks one segment's three buffers (`SPEC.md` §1.1 "Guest input": `fixed` and `claims` as
/// JSON, `proof` as `bincode` 1.3.3) and returns its public values.
pub fn check_segment(
    fixed: &[u8],
    claims: &[u8],
    proof: &[u8],
) -> Result<SegmentPublicValues, Reject> {
    let fixed: Fixed =
        serde_json::from_slice(fixed).map_err(|e| Reject::Malformed(e.to_string()))?;
    let (segment_index, plan) = check_statement(&fixed)?;
    // As upstream's byte entry: undecodable claims or proof are a rejected verdict.
    let claims: Claims =
        serde_json::from_slice(claims).map_err(|e| undecodable("claims", e.to_string()))?;
    let proof: Proof =
        bincode::deserialize(proof).map_err(|e| undecodable("proof", e.to_string()))?;
    let verdict = verify_composed_parsed(&fixed, &plan, &claims, proof);
    check_verdict(segment_index, &fixed, &plan, &verdict)
}

fn undecodable(what: &str, e: String) -> Reject {
    Reject::NotAccepted {
        failed: vec![],
        error: Some(format!("unparseable {what}: {e}")),
    }
}

/// Rules 1 to 5 on the parsed statement. Returns `segmentIndex` and the statement's `Plan`, the
/// only one built: `verify_composed_parsed` draws its relations and challenges from it.
pub fn check_statement(fixed: &Fixed) -> Result<(u16, Plan), Reject> {
    token_ids_in_field(fixed)?;
    security_params(fixed)?;
    let segment_index = segment_index(fixed)?;
    let plan = Plan::new(&fixed.params);
    edges_bound(fixed, &plan.binding(fixed.layer))?;
    dequant_sound(fixed.params.kmax, &plan.specs)?;
    Ok((segment_index, plan))
}

/// Rules 6 and 7 on the verifier build's `Verdict` for the statement `check_statement` passed,
/// then the accepted segment's public values: the only way `check_segment` returns them.
pub fn check_verdict(
    segment_index: u16,
    fixed: &Fixed,
    plan: &Plan,
    verdict: &Verdict,
) -> Result<SegmentPublicValues, Reject> {
    verdict_rules(verdict)?;
    Ok(public_values(segment_index, fixed, plan, verdict))
}

fn verdict_rules(verdict: &Verdict) -> Result<(), Reject> {
    accepted(verdict)?;
    seams_closed(verdict)
}

/// `fixedHash`: `sha256` of the re-serialized parsed statement with `tok_in` and `tok_out`
/// cleared, never of the input bytes.
pub fn fixed_hash(fixed: &Fixed) -> [u8; 32] {
    let cleared = Fixed {
        tok_in: vec![],
        tok_out: vec![],
        ..fixed.clone()
    };
    Sha256::digest(serde_json::to_vec(&cleared).expect("Fixed serializes")).into()
}

fn token_ids_in_field(fixed: &Fixed) -> Result<(), Reject> {
    for (vector, ids) in [("tok_in", &fixed.tok_in), ("tok_out", &fixed.tok_out)] {
        if let Some((index, &id)) = ids.iter().enumerate().find(|(_, id)| **id >= P) {
            return Err(Reject::TokenIdOutOfField { vector, index, id });
        }
    }
    Ok(())
}

fn security_params(fixed: &Fixed) -> Result<(), Reject> {
    if fixed.log_blowup == LOG_BLOWUP && fixed.n_queries == N_QUERIES && fixed.pow_bits == POW_BITS
    {
        return Ok(());
    }
    Err(Reject::SecurityParams {
        log_blowup: fixed.log_blowup,
        n_queries: fixed.n_queries,
        pow_bits: fixed.pow_bits,
    })
}

fn segment_index(fixed: &Fixed) -> Result<u16, Reject> {
    fixed
        .layer
        .and_then(|layer| u16::try_from(layer).ok())
        .ok_or(Reject::MissingLayer { layer: fixed.layer })
}

/// `binding` is `Plan::binding(fixed.layer)`. The `--gkr-*` backend flags stay allowed.
fn edges_bound(fixed: &Fixed, binding: &[Edge]) -> Result<(), Reject> {
    let edges: Vec<u8> = binding
        .iter()
        .filter(|e| e.state == EdgeState::Unbound)
        .map(|e| e.n)
        .collect();
    let argmax_lax = fixed.params.prove_argmax_lax;
    if edges.is_empty() && !argmax_lax {
        return Ok(());
    }
    Err(Reject::UnboundEdge { edges, argmax_lax })
}

/// `specs` is the plan's dequant `Spec` per matmul.
fn dequant_sound(kmax: u32, specs: &[Spec]) -> Result<(), Reject> {
    let mats: Vec<usize> = specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| !spec.sound())
        .map(|(mi, _)| mi)
        .collect();
    if kmax <= KMAX_SOUND && mats.is_empty() {
        return Ok(());
    }
    Err(Reject::UnsoundDequant { kmax, mats })
}

fn accepted(verdict: &Verdict) -> Result<(), Reject> {
    if verdict.accepts() {
        return Ok(());
    }
    Err(Reject::NotAccepted {
        failed: failed_checks(verdict),
        error: verdict.error.clone(),
    })
}

/// The names of the verdict's `*_ok` and `sums_cancel` checks that are false or hold a false
/// entry, for the rejection message only.
fn failed_checks(verdict: &Verdict) -> Vec<String> {
    let Ok(Value::Object(fields)) = serde_json::to_value(verdict) else {
        return vec![];
    };
    fields
        .into_iter()
        .filter(|(name, value)| {
            (name.ends_with("_ok") || name == "sums_cancel")
                && match value {
                    Value::Bool(ok) => !ok,
                    Value::Array(oks) => oks.contains(&Value::Bool(false)),
                    _ => false,
                }
        })
        .map(|(name, _)| name)
        .collect()
}

fn seams_closed(verdict: &Verdict) -> Result<(), Reject> {
    if verdict.deq_out_open.is_empty()
        && verdict.rq_scale_open.is_empty()
        && !verdict.ao_q_open
        && !verdict.nn_open
    {
        return Ok(());
    }
    Err(Reject::OpenSeam {
        deq_out_open: verdict.deq_out_open.clone(),
        rq_scale_open: verdict.rq_scale_open.clone(),
        ao_q_open: verdict.ao_q_open,
        nn_open: verdict.nn_open,
    })
}

/// The public values of an accepted segment, from its statement, plan and verdict only.
fn public_values(
    segment_index: u16,
    fixed: &Fixed,
    plan: &Plan,
    verdict: &Verdict,
) -> SegmentPublicValues {
    let side = |role| verdict.side_root(Axis::Layer, role, None).map(root);
    SegmentPublicValues {
        claim: SegmentClaim {
            segment_index,
            // Equal to proof commitments 0 and 1 once accepted (`root_ok`, `model_root_ok`).
            pre_root: root(&fixed.pre_root),
            model_root: root(&fixed.model_root),
            residual_in_root: side("in")
                .expect("a segment with every edge bound commits its residual-stream in side"),
            // The head commits no out side.
            residual_out_root: side("out").unwrap_or([0; 32]),
        },
        fixed_hash: fixed_hash(fixed),
        tok_in_hash: match plan.emb {
            Some(_) => compute_token_hash(&fixed.tok_in),
            None => [0; 32],
        },
        tok_out_hash: match plan.argmax_mat() {
            Some(_) => compute_token_hash(&fixed.tok_out),
            None => [0; 32],
        },
    }
}

/// A `Blake2sHash` as the 32 bytes behind its 64-hex-digit `Debug` form.
fn root(hex: &str) -> [u8; 32] {
    let mut out = [0; 32];
    hex::decode_to_slice(hex, &mut out).expect("an accepted root is 64 hex digits");
    out
}

#[cfg(test)]
mod tests;
