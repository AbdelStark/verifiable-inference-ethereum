//! The extracted verifier against upstream's (`sp1/sash-oracle`: upstream `verify_composed`, default
//! features, no calibration), on every corpus bundle and every mutation of `common::MUTATIONS`. Run
//! in the `sash` CI job and by `make sash-synthetic`, with `SASH_SYNTHETIC_CORPUS` and, for the
//! live oracle, `SASH_ORACLE` (the built `sash-oracle` binary).

mod common;

use std::collections::BTreeSet;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use common::{BUNDLES, Bundle, MUTATIONS, Outcome, oracle_records};
use sash_segment_check::{P, fixed_hash};
use serde_json::{Value, json};
use zk_prover::compose::{Fixed, Plan, verify_composed};

/// What `sash-oracle` prints for one bundle, computed with the extracted (no-`prover`)
/// `verify_composed`: its `Verdict` without timing fields, `accepts()`, the binding table, and the
/// guest's `fixed_hash` of the verifier build's `Fixed`.
fn extracted(bundle: &Bundle) -> Value {
    let out = panic::catch_unwind(AssertUnwindSafe(|| {
        let verdict = verify_composed(&bundle.fixed, &bundle.claims, &bundle.proof);
        let accepts = verdict.accepts();
        let mut verdict = serde_json::to_value(&verdict).unwrap();
        // `verify_secs` is not in the verifier build; `shape_secs` is the other timing field.
        verdict.as_object_mut().unwrap().remove("shape_secs");
        let parsed = serde_json::from_slice::<Fixed>(&bundle.fixed).ok();
        let binding = parsed
            .as_ref()
            .map(|f| Plan::new(&f.params).binding_json(f.layer));
        let fixed_hash = parsed.map(|f| hex::encode(fixed_hash(&f)));
        json!({ "verdict": verdict, "accepts": accepts, "binding": binding, "fixedHash": fixed_hash })
    }));
    out.unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "non-string panic payload".into());
        json!({ "panic": message })
    })
}

fn oracle(bin: &Path, name: &str, bundle: &Bundle) -> Value {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("differential");
    fs::create_dir_all(&dir).unwrap();
    for (kind, bytes) in [
        ("fixed", &bundle.fixed),
        ("claims", &bundle.claims),
        ("proof", &bundle.proof),
    ] {
        fs::write(dir.join(format!("{name}.{kind}.json")), bytes).unwrap();
    }
    let out = Command::new(bin).arg(&dir).arg(name).output().unwrap();
    assert!(out.status.success(), "sash-oracle {name}: {out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

/// `SPEC.md` §1.1's rules beyond upstream's `accepts()`, read from the statement and the oracle's
/// output rather than from `check_segment`.
fn rules_beyond_accepts(bundle: &Bundle, reference: &Value) -> bool {
    let Ok(fixed) = serde_json::from_slice::<Fixed>(&bundle.fixed) else {
        return false;
    };
    let verdict = &reference["verdict"];
    let unbound = reference["binding"]["unbound"].as_array().unwrap();
    let specs_sound = Plan::new(&fixed.params).specs.iter().all(|s| s.sound());
    fixed.tok_in.iter().chain(&fixed.tok_out).all(|id| *id < P)
        && (fixed.log_blowup, fixed.n_queries, fixed.pow_bits) == (1, 100, 0)
        && fixed.layer.is_some_and(|l| l < 1 << 16)
        && unbound.is_empty()
        && !fixed.params.prove_argmax_lax
        && fixed.params.kmax <= 21
        && specs_sound
        && verdict["deq_out_open"] == json!([])
        && verdict["rq_scale_open"] == json!([])
        && verdict["ao_q_open"] == json!(false)
        && verdict["nn_open"] == json!(false)
        && fixed.block == Some(0)
        && fixed.params.c == 0
}

fn cases() -> Vec<(String, Bundle)> {
    BUNDLES
        .iter()
        .map(|tag| (tag.to_string(), Bundle::load(tag)))
        .chain(MUTATIONS.iter().map(|m| (m.name.to_string(), m.bundle())))
        .collect()
}

/// The extracted `verify_composed` reproduces upstream's committed records
/// (`sp1/testdata/synthetic.oracle.json`) on every corpus bundle.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sash CI job)"]
fn extracted_verifier_matches_the_oracle_records() {
    for tag in BUNDLES {
        assert_eq!(
            extracted(&Bundle::load(tag)),
            oracle_records()[tag],
            "{tag}"
        );
    }
}

/// On every bundle and mutation: the extracted verifier's output equals the live oracle's, and
/// `check_segment` accepts exactly when the oracle's `accepts()` and the rules beyond it hold.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS and sash-oracle in SASH_ORACLE (sash CI job)"]
fn check_segment_agrees_with_the_oracle() {
    let bin = PathBuf::from(
        env::var_os("SASH_ORACLE").expect("SASH_ORACLE must name the built sash-oracle binary"),
    );
    let mut stricter = BTreeSet::new();
    for (name, bundle) in cases() {
        let reference = oracle(&bin, &name, &bundle);
        assert_eq!(extracted(&bundle), reference, "{name}: extracted verifier");
        assert!(
            reference.get("panic").is_none(),
            "{name}: the oracle panicked"
        );
        assert_eq!(reference["verdict"]["shape_root_ok"], Value::Null, "{name}");

        let accepts = reference["accepts"] == json!(true);
        let expected = accepts && rules_beyond_accepts(&bundle, &reference);
        let outcome = bundle.check();
        eprintln!(
            "{name}: oracle accepts {accepts}, check_segment {}",
            outcome.variant()
        );
        assert_eq!(
            matches!(outcome, Outcome::Accept(_)),
            expected,
            "{name}: {outcome:?}"
        );
        if accepts && !expected {
            stricter.insert(name);
        }
    }
    // Where upstream accepts and the guest does not: the unbound bundles, the bundles and mutations
    // with a prefix (rule 8), and the mutations that only SPEC.md §1.1's own rules refuse. Not the
    // three `UnsoundDequant` mutations: `kmax` and the dequant variants size the tables and claims
    // the AIR reads, so upstream rejects those statements on its own (`kmax_22`: `sizes_ok` and `stark_ok` false, `OodsNotMatching`; the
    // other two: `claims shape mismatch`); `UnsoundDequant` is the first rule to see them.
    let want: BTreeSet<String> = [
        "sound_aligned",
        "aligned_gkr5",
        "layer_1_to_2",
        "sound_gkrfive",
        "skip_residual_stream",
        "skip_layer_boundary",
        "skip_kv_boundary",
        "skip_embedding",
        "head",
        "layer_removed",
        "layer_65536",
        "argmax_lax",
        "layer_1_to_0",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(stricter, want);
}
