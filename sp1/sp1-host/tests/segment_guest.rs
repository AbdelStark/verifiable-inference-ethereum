//! The segment guest in the SP1 executor on the synthetic corpus (`SPEC.md` §1.1 and the guest rows
//! of §2.1): every corpus bundle and every bundle mutation of `sash-segment-check`'s native tests,
//! whose mutations this suite shares. Positives exit 0 with the native `check_segment`'s public
//! values, byte for byte; controls exit 1 with a panic message naming the native test's `Reject`
//! variant. The `Verdict`-level and other unit-level controls stay native-only. Run by
//! `make sp1-guest-suite` and in the `sp1` or `sp1-cached` CI job:
//! `SASH_SYNTHETIC_CORPUS=<dir> cargo test --release --locked -p sp1-host --test segment_guest -- --ignored`.

#[path = "../../sash-segment-check/tests/common/mod.rs"]
mod common;

use common::{Bundle, MUTATIONS, Outcome};
use sash_evm_primitives::segment::SEGMENT_PUBLIC_VALUES_LEN;
use sp1_host::{Execution, SEGMENT_GUEST_ELF, SegmentInput, execute};

/// The guest on the three buffers `sp1-host execute` would give it for `bundle`.
fn run(bundle: &Bundle) -> Execution {
    run_buffers(SegmentInput {
        fixed: bundle.fixed.clone(),
        claims: bundle.claims.clone(),
        proof: bundle.proof_bincode(),
    })
}

fn run_buffers(input: SegmentInput) -> Execution {
    execute(SEGMENT_GUEST_ELF, input.stdin()).expect("executor error")
}

/// Exit 0, and the public values `check_segment` returns natively for the same bundle.
fn accepts_as_native(name: &str, bundle: &Bundle) -> Vec<u8> {
    let run = run(bundle);
    assert_eq!(run.exit_code, 0, "{name}: stderr: {}", run.stderr);
    let Outcome::Accept(native) = bundle.check() else {
        panic!("{name}: the native check does not accept");
    };
    assert_eq!(run.public_values.len(), SEGMENT_PUBLIC_VALUES_LEN, "{name}");
    assert_eq!(run.public_values, native.encode(), "{name}");
    eprintln!("{name}: exit 0, {} cycles, {} PGU", run.cycles, run.gas);
    run.public_values
}

/// Exit 1, nothing committed, and a panic message naming `variant`.
fn rejects_with(name: &str, run: &Execution, variant: &str) {
    assert_eq!(run.exit_code, 1, "{name}: stderr: {}", run.stderr);
    assert!(run.public_values.is_empty(), "{name}");
    assert_eq!(
        run.reject(),
        Some(variant),
        "{name}: stderr: {}",
        run.stderr
    );
    eprintln!("{name}: exit 1, {variant}, {} cycles", run.cycles);
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn sound_aligned_accepts() {
    accepts_as_native("sound_aligned", &Bundle::load("sound_aligned"));
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn aligned_gkr5_accepts() {
    accepts_as_native("aligned_gkr5", &Bundle::load("aligned_gkr5"));
}

/// The bundles that publish a token vector (`sp1/patches/0007`), whose forgeries are G9's mutations.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn token_bundles_accept() {
    for tag in ["embed", "head_argmax"] {
        accepts_as_native(tag, &Bundle::load(tag));
    }
}

/// The corpus bundles with an unbound edge (`SPEC.md` §2.1 G5).
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn unbound_bundles_reject() {
    for tag in [
        "sound_gkrfive",
        "skip_residual_stream",
        "skip_layer_boundary",
        "skip_kv_boundary",
        "skip_embedding",
        "head",
    ] {
        rejects_with(tag, &run(&Bundle::load(tag)), "UnboundEdge");
    }
}

/// Every mutation that the native tests expect to reject with `variant` exits 1 naming it.
fn mutations_reject_with(variant: &str) {
    let mut n = 0;
    for m in MUTATIONS.iter().filter(|m| m.expect == variant) {
        n += 1;
        rejects_with(m.name, &run(&m.bundle()), variant);
    }
    assert!(n > 0, "no mutation expects {variant}");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_malformed() {
    mutations_reject_with("Malformed");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_token_id_out_of_field() {
    mutations_reject_with("TokenIdOutOfField");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_security_params() {
    mutations_reject_with("SecurityParams");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_missing_layer() {
    mutations_reject_with("MissingLayer");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_unbound_edge() {
    mutations_reject_with("UnboundEdge");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_unsound_dequant() {
    mutations_reject_with("UnsoundDequant");
}

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_not_accepted() {
    mutations_reject_with("NotAccepted");
}

/// A `proof` buffer that does not decode as `bincode` rejects at rule 6, in the guest's own decode
/// rather than halting in `io::read`.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn undecodable_proof_is_not_accepted() {
    let b = Bundle::load("sound_aligned");
    let mut proof = b.proof_bincode();
    proof.truncate(proof.len() / 2);
    let input = SegmentInput {
        fixed: b.fixed,
        claims: b.claims,
        proof,
    };
    let run = run_buffers(input);
    rejects_with("proof_truncated", &run, "NotAccepted");
    // The guest's `{reject:?}` line, before its panic: the native test's decode error.
    assert!(
        run.stderr.contains("error: Some(\"unparseable proof"),
        "proof_truncated: stderr: {}",
        run.stderr
    );
}

/// `layer` 1 to 2 on `sound_aligned` verifies, with the native public values: the new
/// `segmentIndex` and `fixedHash`, the rest unchanged.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn layer_1_to_2_accepts_with_another_index_and_fixed_hash() {
    let m = MUTATIONS.iter().find(|m| m.name == "layer_1_to_2").unwrap();
    assert_eq!(m.expect, "accept");
    let got = accepts_as_native(m.name, &m.bundle());
    let Outcome::Accept(original) = Bundle::load(m.base).check() else {
        panic!("{} must verify natively", m.base);
    };
    let original = original.encode();
    assert_eq!(got[0..2], [0, 2]);
    assert_eq!(got[2..130], original[2..130]);
    assert_ne!(got[130..162], original[130..162]);
    assert_eq!(got[162..], original[162..]);
}
