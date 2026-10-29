//! The segment guest in the SP1 executor on the synthetic corpus (`SPEC.md` §1.1 and the guest rows
//! of §2.1): every corpus bundle and every bundle mutation of `sash-segment-check`'s native tests,
//! whose mutations this suite shares. Each exits 1 with a panic message naming the native test's
//! `Reject` variant: no corpus bundle is the single token block `B0` (rule 8), so none exits 0 here
//! (`AGENTS.md` §9); `make sp1-execute` runs the accepting TinyLlama segments. The `Verdict`-level
//! and other unit-level controls stay native-only. Run by `make sp1-guest-suite` and in the `sp1` or
//! `sp1-cached` CI job:
//! `SASH_SYNTHETIC_CORPUS=<dir> cargo test --release --locked -p sp1-host --test segment_guest -- --ignored`.

#[path = "../../sash-segment-check/tests/common/mod.rs"]
mod common;

use common::{Bundle, MUTATIONS};
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

/// The corpus bundles that pass rules 1 to 7 and publish no token vector: block 0 with a 128-key
/// prefix (`SPEC.md` §2.1 G10). The guest verifies them and rejects them at the last rule.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn prefix_bundles_reject() {
    for tag in ["sound_aligned", "aligned_gkr5"] {
        rejects_with(tag, &run(&Bundle::load(tag)), "NotSingleBlock");
    }
}

/// The bundles that publish a token vector (`sp1/patches/0007`), whose forgeries are G9's mutations:
/// the guest verifies them, token binding included, and rejects them at the last rule (`SPEC.md`
/// §2.1 G9, G10). A guest whose verifier dropped patch 0006's mix would reject them at rule 6.
#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn token_bundles_verify() {
    for tag in ["embed", "head_argmax"] {
        let run = run(&Bundle::load(tag));
        rejects_with(tag, &run, "NotSingleBlock");
        // The guest's `{reject:?}` line, before its panic: block 0 with a 128-key prefix.
        assert!(
            run.stderr
                .contains("NotSingleBlock { block: Some(0), prefix: 128 }"),
            "{tag}: stderr: {}",
            run.stderr
        );
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

#[test]
#[ignore = "needs the synthetic corpus in SASH_SYNTHETIC_CORPUS (sp1 or sp1-cached CI job)"]
fn mutations_not_single_block() {
    mutations_reject_with("NotSingleBlock");
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
