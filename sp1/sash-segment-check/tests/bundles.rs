//! `check_segment` on the synthetic corpus (`SPEC.md` §1.1 and the guest rows of §2.1). Run in the
//! `sash` CI job and by `make sash-synthetic`:
//! `SASH_SYNTHETIC_CORPUS=<dir> cargo test --locked -p sash-segment-check -- --ignored`.

mod common;

use common::{Bundle, MUTATIONS, Outcome, oracle_records};
use sash_segment_check::{Reject, SegmentPublicValues, check_statement, check_verdict};
use sha2::{Digest, Sha256};
use zk_prover::common::Proof;
use zk_prover::compose::{Claims, Fixed, Verdict, verify_composed_parsed};

/// The public values of the three bundles that pass rules 1 to 7, pinned. `derived_public_values`
/// re-derives them without the segment check. `check_segment` returns `single_block`'s (block 0, no
/// prefix) and rejects the other two at rule 8: they are block 0 with a 128-key prefix
/// (`--ctx 128`), and `check_verdict` gives theirs.
const SINGLE_BLOCK: &str = "TODO";
const SOUND_ALIGNED: &str = "0001a215b612fae4857872a1bd4411b90a4a82f5e44c345760717ef3ea2936cf76177bf74a44901bf3333e8fd63148615062be8f58546b902c5f687d0b479739154728211a225a72e4084c4e004cbf447936ee80ec6fb7ee807a68263101e01e6b2fe3af1f6b83609f513230b12f4b194b10d4fcfa1c08a8b253745f9e67edb0d942f3b0dd78edd308a62a56b952819cc91ca6dfbab3945da6b8036d2995d5deef7000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const ALIGNED_GKR5: &str = "0001aa02a375b333326a89a9650203c58340f56ae7443c83960661840f474daf366b7bf74a44901bf3333e8fd63148615062be8f58546b902c5f687d0b479739154728211a225a72e4084c4e004cbf447936ee80ec6fb7ee807a68263101e01e6b2fe3af1f6b83609f513230b12f4b194b10d4fcfa1c08a8b253745f9e67edb0d94218d5d7714086769726175164b6a7edc9a23da972f72512e02d4042cff7f5bdcb00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

/// The public values `SPEC.md` §1.1's table gives for a bundle that passes rules 1 to 7, laid out by
/// hand from:
/// - `segmentIndex`, `preRoot`, `modelRoot`: `fixed.json`'s `layer`, `pre_root`, `model_root`;
/// - `residualInRoot`, `residualOutRoot`: the `(layer, in)` and `(layer, out)` roots of upstream's
///   `boundary_sides` in `sp1/testdata/synthetic.oracle.json`;
/// - `fixedHash`: sha256 of the parsed `Fixed`, `tok_in` and `tok_out` cleared, re-serialized;
/// - `tokInHash`, `tokOutHash`: zero, since the oracle's binding table has neither the embedding
///   (edge 15) nor the argmax (edge 16).
fn derived_public_values(tag: &str) -> String {
    let bundle = Bundle::load(tag);
    let fixed = bundle.fixed_json();
    let record = &oracle_records()[tag];
    let absent: Vec<u64> = serde_json::from_value(record["binding"]["absent"].clone()).unwrap();
    assert!(absent.contains(&15) && absent.contains(&16));
    assert!(fixed.get("tok_in").is_none() && fixed.get("tok_out").is_none());

    let layer_side = |role: &str| -> String {
        let sides = record["verdict"]["boundary_sides"].as_array().unwrap();
        let side = sides
            .iter()
            .find(|s| s["axis"] == "layer" && s["role"] == role && s["group"].is_null())
            .unwrap_or_else(|| panic!("{tag}: no (layer, {role}) side"));
        side["root"].as_str().unwrap().to_string()
    };
    let parsed: Fixed = serde_json::from_slice(&bundle.fixed).unwrap();
    let cleared = Fixed {
        tok_in: vec![],
        tok_out: vec![],
        ..parsed
    };
    let fixed_hash = hex::encode(Sha256::digest(serde_json::to_vec(&cleared).unwrap()));

    let layer = u16::try_from(fixed["layer"].as_u64().unwrap()).unwrap();
    [
        hex::encode(layer.to_be_bytes()),
        fixed["pre_root"].as_str().unwrap().to_string(),
        fixed["model_root"].as_str().unwrap().to_string(),
        layer_side("in"),
        layer_side("out"),
        fixed_hash,
        "00".repeat(32),
        "00".repeat(32),
    ]
    .concat()
}

/// `bundle`'s statement, and its public values from `check_verdict`, which passes rules 6 and 7
/// before `check_segment` applies rule 8.
fn verified(bundle: &Bundle) -> (Fixed, Result<SegmentPublicValues, Reject>) {
    let fixed: Fixed = serde_json::from_slice(&bundle.fixed).unwrap();
    let claims: Claims = serde_json::from_slice(&bundle.claims).unwrap();
    let proof: Proof = serde_json::from_slice(&bundle.proof).unwrap();
    let (segment_index, plan) = check_statement(&fixed).unwrap();
    let verdict = verify_composed_parsed(&fixed, &plan, &claims, proof);
    let values = check_verdict(segment_index, &fixed, &plan, &verdict);
    (fixed, values)
}

/// Every rule holds: block 0 with no prefix, and the pinned public values.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn single_block_accepts() {
    assert_eq!(derived_public_values("single_block"), SINGLE_BLOCK);
    let bundle = Bundle::load("single_block");
    let fixed: Fixed = serde_json::from_slice(&bundle.fixed).unwrap();
    assert_eq!((fixed.block, fixed.params.c), (Some(0), 0));
    match bundle.check() {
        Outcome::Accept(values) => assert_eq!(hex::encode(values.encode()), SINGLE_BLOCK),
        other => panic!("single_block: expected acceptance, got {other:?}"),
    }
}

/// Rules 1 to 7 hold and give the pinned public values; rule 8 rejects: block 0, prefix 128.
fn passes_rules_1_to_7_then_not_single_block(tag: &str, pinned: &str) {
    assert_eq!(derived_public_values(tag), pinned);
    let bundle = Bundle::load(tag);
    let (fixed, values) = verified(&bundle);
    assert_eq!(hex::encode(values.unwrap().encode()), pinned, "{tag}");
    assert_eq!((fixed.block, fixed.params.c), (Some(0), 128), "{tag}");
    match bundle.check() {
        Outcome::Reject(Reject::NotSingleBlock { block, prefix }) => {
            assert_eq!((block, prefix), (Some(0), 128), "{tag}");
        }
        other => panic!("{tag}: expected NotSingleBlock, got {other:?}"),
    }
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn sound_aligned_is_not_single_block() {
    passes_rules_1_to_7_then_not_single_block("sound_aligned", SOUND_ALIGNED);
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn aligned_gkr5_is_not_single_block() {
    passes_rules_1_to_7_then_not_single_block("aligned_gkr5", ALIGNED_GKR5);
}

/// The bundles upstream composes with an `--unsound-*` flag, or `sound_gkrfive`'s narrow shape:
/// `UnboundEdge` with exactly the oracle's unbound edges.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn unbound_bundles_reject() {
    for tag in [
        "sound_gkrfive",
        "skip_residual_stream",
        "skip_layer_boundary",
        "skip_kv_boundary",
        "skip_embedding",
        "head",
    ] {
        let want: Vec<u8> =
            serde_json::from_value(oracle_records()[tag]["binding"]["unbound"].clone()).unwrap();
        assert!(
            !want.is_empty(),
            "{tag}: the oracle reports no unbound edge"
        );
        match Bundle::load(tag).check() {
            Outcome::Reject(sash_segment_check::Reject::UnboundEdge { edges, argmax_lax }) => {
                assert_eq!(edges, want, "{tag}");
                assert!(!argmax_lax, "{tag}");
            }
            other => panic!("{tag}: expected UnboundEdge, got {other:?}"),
        }
    }
}

/// Every mutation of `common::MUTATIONS` that expects `variant` gets it.
fn mutations_reject_with(variant: &str) {
    let mut wrong = vec![];
    let mut n = 0;
    for m in MUTATIONS.iter().filter(|m| m.expect == variant) {
        n += 1;
        let outcome = m.bundle().check();
        eprintln!("{}: {}", m.name, outcome.variant());
        if outcome.variant() != variant {
            wrong.push(format!("{}: expected {variant}, got {outcome:?}", m.name));
        }
    }
    assert!(n > 0, "no mutation expects {variant}");
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_malformed() {
    mutations_reject_with("Malformed");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_token_id_out_of_field() {
    mutations_reject_with("TokenIdOutOfField");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_security_params() {
    mutations_reject_with("SecurityParams");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_missing_layer() {
    mutations_reject_with("MissingLayer");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_unbound_edge() {
    mutations_reject_with("UnboundEdge");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_unsound_dequant() {
    mutations_reject_with("UnsoundDequant");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_not_accepted() {
    mutations_reject_with("NotAccepted");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn mutations_not_single_block() {
    mutations_reject_with("NotSingleBlock");
}

/// `layer` 1 to 2 on `single_block` is still accepted: the statement's layer is read by the binding
/// table only. Its public values carry it, and `fixedHash` moves with it.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn layer_1_to_2_accepts_with_another_index_and_fixed_hash() {
    let m = MUTATIONS.iter().find(|m| m.name == "layer_1_to_2").unwrap();
    assert_eq!((m.base, m.expect), ("single_block", "accept"));
    let Outcome::Accept(values) = m.bundle().check() else {
        panic!("layer 1 to 2 must be accepted");
    };
    let original = hex::decode(SINGLE_BLOCK).unwrap();
    let encoded = values.encode();
    assert_eq!(encoded[0..2], [0, 2]);
    assert_eq!(encoded[2..130], original[2..130]);
    assert_ne!(encoded[130..162], original[130..162]);
    assert_eq!(encoded[162..], original[162..]);
}

/// A `proof` buffer that does not decode as `bincode` rejects at rule 6, as upstream's byte entry
/// rejects an unparseable proof.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn undecodable_proof_is_not_accepted() {
    let b = Bundle::load("sound_aligned");
    let mut proof = b.proof_bincode();
    proof.truncate(proof.len() / 2);
    let got = sash_segment_check::check_segment(&b.fixed, &b.claims, &proof);
    assert!(
        matches!(&got, Err(sash_segment_check::Reject::NotAccepted { error: Some(e), .. }) if e.starts_with("unparseable proof")),
        "{got:?}"
    );
}

/// Rule 7 after the verifier, which no corpus bundle reaches with an open seam: `sound_aligned`'s
/// own `Plan` and `Verdict` give its public values through `check_verdict`, the step
/// `check_segment` returns them from, and `OpenSeam` once any one seam is opened, though upstream's
/// `accepts()` still holds.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn open_seam_after_the_verifier() {
    let b = Bundle::load("sound_aligned");
    let fixed: Fixed = serde_json::from_slice(&b.fixed).unwrap();
    let claims: Claims = serde_json::from_slice(&b.claims).unwrap();
    let proof: Proof = serde_json::from_slice(&b.proof).unwrap();
    let (segment_index, plan) = check_statement(&fixed).unwrap();
    let verdict = verify_composed_parsed(&fixed, &plan, &claims, proof);
    let values = check_verdict(segment_index, &fixed, &plan, &verdict).unwrap();
    assert_eq!(hex::encode(values.encode()), SOUND_ALIGNED);

    let opens: [fn(&mut Verdict); 4] = [
        |v| v.deq_out_open = vec!["q".into()],
        |v| v.rq_scale_open = vec!["qkv/v".into()],
        |v| v.ao_q_open = true,
        |v| v.nn_open = true,
    ];
    for open in opens {
        let mut v = verdict.clone();
        open(&mut v);
        assert!(v.accepts(), "upstream's accepts() ignores open seams");
        assert!(
            matches!(
                check_verdict(segment_index, &fixed, &plan, &v),
                Err(Reject::OpenSeam { .. })
            ),
            "{v:?}"
        );
    }
}
