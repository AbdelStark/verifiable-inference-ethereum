//! The committed PLONK fixture, `contracts/fixtures/tinyllama.plonk.json` (`SPEC.md` §1.2, §1.4): the
//! aggregator's PLONK proof over the 23 TinyLlama segment proofs, as `sp1-host aggregate
//! --fixture-out` wrote it. It must carry the aggregator's pinned vkey and §1.4's `publicDigest`, and
//! verify offline with the v6.1.0 PLONK verifying key `sp1-verifier` embeds, the one the pinned
//! `SP1VerifierPlonk` was generated from (`SECURITY.md`). Needs no download and no GPU:
//! `cargo test --workspace`.

use std::path::PathBuf;

use sha2::{Digest, Sha256};
use sp1_host::aggregate::{check_plonk_proof_bytes, plonk_fixture_command, plonk_fixture_json};
use sp1_host::pins::{AGGREGATOR_GUEST, repo_root};
use sp1_sdk::{SP1Proof, SP1ProofWithPublicValues, SP1PublicValues};
use sp1_verifier::{PLONK_VK_BYTES, PlonkBn254Proof, PlonkVerifier, VK_ROOT_BYTES};

/// `SPEC.md` §1.4's `publicDigest`.
const SPEC_PUBLIC_DIGEST: &str =
    "0xcf52e19e82f82680f0098ab435b52e22512f3d634bd7a947d32abc76d067b3ef";

/// `VERIFIER_HASH()` of the pinned v6.1.0 `SP1VerifierPlonk` (`SPEC.md` §1.2, `SECURITY.md`): the
/// sha256 of `PLONK_VK_BYTES`; its first 4 bytes are `proofBytes`' selector.
const VERIFIER_HASH: &str = "5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac";

/// `VK_ROOT()` of the pinned v6.1.0 `SP1VerifierPlonk` (`SPEC.md` §1.2, `SECURITY.md`).
const VK_ROOT: &str = "002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352";

/// `proofBytes`' length (`SPEC.md` §1.2).
const PROOF_BYTES_LEN: usize = 964;

/// The fixture's three fields, as written.
struct Fixture {
    program_vkey: String,
    public_values: String,
    proof_bytes: String,
}

fn fixture() -> Fixture {
    let path = repo_root().join("contracts/fixtures/tinyllama.plonk.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("the fixture is not JSON");
    let object = json.as_object().expect("the fixture is not a JSON object");
    let field = |key: &str| object[key].as_str().expect(key).to_string();
    let fixture = Fixture {
        program_vkey: field("programVKey"),
        public_values: field("publicValues"),
        proof_bytes: field("proofBytes"),
    };
    // Byte for byte what `--fixture-out` renders from those fields, so no other key and no other
    // layout: never hand-edited.
    let rendered = plonk_fixture_json(
        &fixture.program_vkey,
        &hex_bytes(&fixture.public_values),
        &hex_bytes(&fixture.proof_bytes),
    );
    assert!(
        text == rendered,
        "the fixture is not what sp1-host aggregate --fixture-out writes from its fields (sha256 {}, rendered {})",
        hex::encode(Sha256::digest(&text)),
        hex::encode(Sha256::digest(&rendered))
    );
    fixture
}

/// A `0x`-prefixed lowercase hex string's bytes.
fn hex_bytes(field: &str) -> Vec<u8> {
    let hex = field.strip_prefix("0x").expect("no 0x prefix");
    assert_eq!(hex, hex.to_lowercase(), "not lowercase hex");
    hex::decode(hex).expect("not hex")
}

/// `programVKey` is the aggregator guest's pinned `bytes32`, `sp1/aggregator-guest.vkey`.
#[test]
fn program_vkey_is_the_aggregator_guests_pinned_vkey() {
    assert_eq!(fixture().program_vkey, AGGREGATOR_GUEST.bytes32().unwrap());
}

/// `publicValues` is `SPEC.md` §1.4's `publicDigest`, the 32 bytes the aggregator commits.
#[test]
fn public_values_are_the_spec_public_digest() {
    assert_eq!(fixture().public_values, SPEC_PUBLIC_DIGEST);
}

/// `proofBytes` are 964 bytes starting with the v6.1.0 selector `0x5a093a2f` (`SPEC.md` §1.2).
#[test]
fn proof_bytes_are_964_bytes_with_the_v6_1_0_selector() {
    let bytes = hex_bytes(&fixture().proof_bytes);
    assert_eq!(bytes.len(), PROOF_BYTES_LEN);
    assert_eq!(hex::encode(&bytes[..4]), VERIFIER_HASH[..8]);
}

/// `sp1-verifier`'s `PlonkVerifier` accepts the fixture with its embedded `PLONK_VK_BYTES`, and so does
/// the stricter check of `SP1VerifierPlonk`'s rules (nonce `0`, sha256 public-values hash); a flipped
/// byte of `publicValues` is rejected.
#[test]
fn proof_verifies_with_the_embedded_plonk_vk() {
    let fixture = fixture();
    let proof = hex_bytes(&fixture.proof_bytes);
    let public_values = hex_bytes(&fixture.public_values);
    PlonkVerifier::verify(
        &proof,
        &public_values,
        &fixture.program_vkey,
        &PLONK_VK_BYTES,
    )
    .expect("PlonkVerifier rejects the fixture");
    check_plonk_proof_bytes(&proof, &public_values, &fixture.program_vkey).unwrap();

    let mut flipped = public_values;
    flipped[0] ^= 1;
    assert!(
        PlonkVerifier::verify(&proof, &flipped, &fixture.program_vkey, &PLONK_VK_BYTES).is_err(),
        "PlonkVerifier accepts the fixture with a flipped public-values byte"
    );
}

/// The `sp1-verifier` the fixture is checked with embeds the v6.1.0 PLONK verifying key and `VK_ROOT`
/// that the pinned `SP1VerifierPlonk` binds (`SPEC.md` §1.2, `SECURITY.md`).
#[test]
fn sp1_verifier_embeds_the_pinned_plonk_vk_and_vk_root() {
    assert_eq!(hex::encode(*VK_ROOT_BYTES), VK_ROOT);
    assert_eq!(hex::encode(Sha256::digest(*PLONK_VK_BYTES)), VERIFIER_HASH);
}

/// `sp1-host plonk-fixture` (`make sp1-proofs-check`) accepts a bincode PLONK proof file that carries
/// the fixture's `proofBytes` and `publicValues`, and rejects one whose public values differ by one byte.
#[test]
fn plonk_fixture_command_binds_a_proof_file_to_the_fixture() {
    let fixture = fixture();
    let proof_bytes = hex_bytes(&fixture.proof_bytes);
    let public_values = hex_bytes(&fixture.public_values);
    let fixture_path = repo_root().join("contracts/fixtures/tinyllama.plonk.json");
    // A proof file as `aggregate --mode plonk --out` writes it: `bytes()` is the first 4 bytes of
    // `plonk_vkey_hash` followed by `encoded_proof`.
    let write = |name: &str, public_values: &[u8]| -> PathBuf {
        let proof = SP1ProofWithPublicValues {
            proof: SP1Proof::Plonk(PlonkBn254Proof {
                public_inputs: Default::default(),
                encoded_proof: hex::encode(&proof_bytes[4..]),
                raw_proof: String::new(),
                plonk_vkey_hash: Sha256::digest(*PLONK_VK_BYTES).into(),
            }),
            public_values: SP1PublicValues::from(public_values),
            sp1_version: String::new(),
            tee_proof: None,
        };
        assert_eq!(proof.bytes(), proof_bytes);
        let path = std::env::temp_dir().join(format!(
            "sp1-host-plonk-fixture-{}-{name}.bin",
            std::process::id()
        ));
        std::fs::write(&path, bincode::serialize(&proof).unwrap()).unwrap();
        path
    };

    let honest = write("honest", &public_values);
    let result = plonk_fixture_command(&honest, &fixture_path);
    std::fs::remove_file(&honest).unwrap();
    result.expect("plonk-fixture rejects the fixture's own proof");

    let mut flipped = public_values;
    flipped[0] ^= 1;
    let mutated = write("flipped", &flipped);
    let result = plonk_fixture_command(&mutated, &fixture_path);
    std::fs::remove_file(&mutated).unwrap();
    assert!(
        result.is_err(),
        "plonk-fixture accepts a proof file with a flipped public-values byte"
    );
}
