//! The aggregator guest in the SP1 executor (`SPEC.md` §1.2), without the segment proofs (deferred
//! proof verification off): it commits §1.4's `publicDigest` on the 23 TinyLlama public values, and
//! controls A1 and A2 (§2.1) exit non-zero naming their rejection. Also the guards around it: its
//! `SEGMENT_VKEY` literal is `sp1/segment-guest.vkey`'s `hash_u32`, and the control-only echo guest
//! of B2 commits its input unchanged under another vkey. Needs no bundle: `cargo test --workspace`.

use sash_evm_primitives::fixture::tinyllama_segment_public_values;
use sash_evm_primitives::tinyllama_fixture::tinyllama_model_id;
use sha2::{Digest, Sha256};
use sp1_host::aggregate::{
    AGGREGATOR_GUEST_SOURCE, aggregator_stdin, execute_aggregator, segment_vkey, vkey_hash_u32,
};
use sp1_host::prove::VKEY;
use sp1_host::{AGGREGATOR_GUEST_ELF, ECHO_GUEST_ELF, Execution, execute};
use sp1_sdk::SP1Stdin;
use sp1_sdk::blocking::{ProverClient, prelude::*};

/// `SPEC.md` §1.4's `publicDigest`.
const SPEC_PUBLIC_DIGEST: &str = "cf52e19e82f82680f0098ab435b52e22512f3d634bd7a947d32abc76d067b3ef";

/// `B0L10`, segment 10 of the TinyLlama run.
const B0L10: usize = 10;

fn run(model_id: &[u8], segment_values: &[u8]) -> Execution {
    execute_aggregator(
        AGGREGATOR_GUEST_ELF,
        aggregator_stdin(model_id, segment_values),
    )
    .expect("executor error")
}

/// Exit 0, and exactly `digest`'s 32 bytes committed.
fn commits(name: &str, run: &Execution, digest: &[u8]) {
    assert_eq!(run.exit_code, 0, "{name}: stderr: {}", run.stderr);
    assert_eq!(
        hex::encode(&run.public_values),
        hex::encode(digest),
        "{name}"
    );
    println!("{name}: {} cycles, {} PGU", run.cycles, run.gas);
}

/// Exit 1, nothing committed, and a panic message naming `variant`.
fn rejects_with(name: &str, run: &Execution, variant: &str) {
    assert_eq!(run.exit_code, 1, "{name}: stderr: {}", run.stderr);
    assert!(run.public_values.is_empty(), "{name}");
    assert_eq!(
        run.aggregate_reject(),
        Some(variant),
        "{name}: stderr: {}",
        run.stderr
    );
}

#[test]
fn tinyllama_commits_the_spec_public_digest() {
    let values = tinyllama_segment_public_values().concat();
    let run = run(&tinyllama_model_id(), &values);
    commits("N = 23", &run, &hex::decode(SPEC_PUBLIC_DIGEST).unwrap());
}

/// `N = 1`: `B0L10`'s values alone commit `sha256(modelId || be32(1) || claim || 0^32 || 0^32)`.
#[test]
fn b0l10_alone() {
    let b0l10 = tinyllama_segment_public_values()[B0L10];
    let mut preimage = tinyllama_model_id().to_vec();
    preimage.extend_from_slice(&1u32.to_be_bytes());
    preimage.extend_from_slice(&b0l10[..162]);
    preimage.extend_from_slice(&[0; 64]);
    let run = run(&tinyllama_model_id(), &b0l10);
    commits("N = 1 (B0L10)", &run, &Sha256::digest(&preimage));
}

/// A1 (`SPEC.md` §2.1): `modelId` of 31 or 33 bytes.
#[test]
fn a1_model_id_length() {
    let values = tinyllama_segment_public_values().concat();
    let mut long = tinyllama_model_id().to_vec();
    long.push(0);
    for (name, model_id) in [("31 bytes", &long[..31]), ("33 bytes", &long[..])] {
        rejects_with(name, &run(model_id, &values), "ModelIdLength");
    }
}

/// A2 (`SPEC.md` §2.1): `segmentValues` empty, or `226 * 23 - 1` bytes.
#[test]
fn a2_segment_values_length() {
    let values = tinyllama_segment_public_values().concat();
    for (name, segment_values) in [
        ("empty", &[][..]),
        ("226 * 23 - 1", &values[..226 * 23 - 1]),
    ] {
        rejects_with(
            name,
            &run(&tinyllama_model_id(), segment_values),
            "SegmentValuesLength",
        );
    }
}

/// The aggregator's `SEGMENT_VKEY` literal is `hash_u32` in `sp1/segment-guest.vkey` (`SPEC.md` §1.2):
/// fails if either moves without the other.
#[test]
fn segment_vkey_is_the_pinned_one() {
    assert_eq!(
        segment_vkey(AGGREGATOR_GUEST_SOURCE).unwrap(),
        vkey_hash_u32(VKEY).unwrap(),
        "the aggregator guest's SEGMENT_VKEY differs from sp1/segment-guest.vkey's hash_u32"
    );
}

/// B2's echo guest (`SPEC.md` §2.1) commits `B0L10`'s 226 public values byte for byte, under a vkey
/// whose `hash_u32` is not `SEGMENT_VKEY`.
#[test]
fn echo_guest_commits_b0l10_under_another_vkey() {
    let b0l10 = tinyllama_segment_public_values()[B0L10];
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(b0l10.to_vec());
    let run = execute(ECHO_GUEST_ELF, stdin).expect("executor error");
    commits("echo B0L10", &run, &b0l10);

    let pk = ProverClient::builder()
        .light()
        .build()
        .setup(ECHO_GUEST_ELF)
        .expect("setup");
    let echo_vkey = pk.verifying_key().hash_u32();
    assert_ne!(echo_vkey, segment_vkey(AGGREGATOR_GUEST_SOURCE).unwrap());
    println!("echo guest hash_u32: {echo_vkey:?}");
}
