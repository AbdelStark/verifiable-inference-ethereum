//! The GPU proving commands' refusals that need no proof and no GPU (`docs/EVM-GAS-MODEL.md` §9): the
//! guests' pins, the tags, the expected public values, what a resumable `<tag>.proof.bin` must be, and
//! the out paths, as functions and through the `sp1-host prove`, `aggregate` and `aggregate-control`
//! commands, which exit before any CUDA client starts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sash_evm_primitives::segment::SegmentPublicValues;
use sp1_host::aggregate::{Mode, check_aggregate_args, check_plonk_proof, plonk_fixture_json};
use sp1_host::pins::{
    AGGREGATOR_GUEST, SEGMENT_GUEST, decode_segment_proof, expected_public_values, parse_tags,
    repeated_tag, repo_root,
};
use sp1_host::prove::{check_new_file, check_new_path, check_out_dir, create_new, existing_proof};
use sp1_host::{read_fixed_pins, tinyllama_tags};
use sp1_sdk::blocking::prelude::*;
use sp1_sdk::{SP1Proof, SP1PublicValues};
use sp1_verifier::PlonkBn254Proof;

/// A fresh empty directory under the system's temp directory, outside the checkout.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sp1-host-prove-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Runs `sp1-host` with `args`: its exit code and stderr.
fn sp1_host(args: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_sp1-host"))
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A PLONK proof as the SDK encodes one: `bytes()` is the 4-byte `plonk_vkey_hash` prefix, then
/// `encoded_proof`.
fn plonk_proof(
    selector: [u8; 4],
    encoded_len: usize,
    public_values: &[u8],
) -> SP1ProofWithPublicValues {
    let mut plonk_vkey_hash = [0; 32];
    plonk_vkey_hash[..4].copy_from_slice(&selector);
    SP1ProofWithPublicValues::new(
        SP1Proof::Plonk(PlonkBn254Proof {
            encoded_proof: hex::encode(vec![1; encoded_len]),
            plonk_vkey_hash,
            ..Default::default()
        }),
        SP1PublicValues::from(public_values),
        "v6.1.0".into(),
    )
}

// --- The guests' pins.

#[test]
fn elf_off_its_pin_is_rejected() {
    for pins in [SEGMENT_GUEST, AGGREGATOR_GUEST] {
        let err = pins.check_elf(b"not the guest").unwrap_err();
        assert!(
            err.contains(&format!("sp1/{}.elf.sha256", pins.name)),
            "{err}"
        );
    }
}

#[test]
fn committed_vkeys_are_accepted() {
    SEGMENT_GUEST.check_vkey(SEGMENT_GUEST.vkey).unwrap();
    AGGREGATOR_GUEST.check_vkey(AGGREGATOR_GUEST.vkey).unwrap();
    // Each guest's pin is its own.
    SEGMENT_GUEST.check_vkey(AGGREGATOR_GUEST.vkey).unwrap_err();
}

/// The committed vkey with one field changed: the last hex digit of `bytes32`, or `hash_u32[0] + 1`.
#[test]
fn another_vkey_is_rejected() {
    let vkey = SEGMENT_GUEST.vkey;
    let (bytes32, hash_u32) = vkey.split_once('\n').unwrap();
    let flipped = match bytes32.chars().last().unwrap() {
        '0' => '1',
        _ => '0',
    };
    let other = format!("{}{flipped}\n{hash_u32}", &bytes32[..bytes32.len() - 1]);
    assert_ne!(other, vkey);
    let err = SEGMENT_GUEST.check_vkey(&other).unwrap_err();
    assert!(err.contains("sp1/segment-guest.vkey"), "{err}");

    let (head, rest) = hash_u32.split_once('[').unwrap();
    let (first, tail) = rest.split_once(',').unwrap();
    let raised = first.parse::<u64>().unwrap() + 1;
    let other = format!("{bytes32}\n{head}[{raised},{tail}");
    assert_ne!(other, vkey);
    SEGMENT_GUEST.check_vkey(&other).unwrap_err();
}

// --- Tags and expected public values.

#[test]
fn tags_are_all_or_known_tags_in_the_order_given() {
    assert_eq!(parse_tags("all").unwrap(), tinyllama_tags());
    assert_eq!(parse_tags("all").unwrap().len(), 23);
    assert_eq!(parse_tags("head,B0L10").unwrap(), ["head", "B0L10"]);
    assert_eq!(parse_tags("B0L10,B0L10").unwrap(), ["B0L10", "B0L10"]);
    for spec in ["", "B0L22", "B0L10,", "HEAD", "B0L10,all", "b0l10"] {
        let err = parse_tags(spec).unwrap_err();
        assert!(err.contains("unknown tag"), "{spec:?}: {err}");
    }
    assert_eq!(
        repeated_tag(&parse_tags("B0L0,head,B0L0").unwrap()),
        Some("B0L0")
    );
    assert_eq!(repeated_tag(&tinyllama_tags()), None);
}

/// Each tag's expected public values carry its index and the `fixedHash` pinned in
/// `sp1/pins/tinyllama.fixed.sha256`, the primary source the segment guest's runs on the bundles wrote.
#[test]
fn expected_public_values_carry_each_segment_index_and_pinned_fixed_hash() {
    let pins = read_fixed_pins(&repo_root().join("sp1/pins/tinyllama.fixed.sha256")).unwrap();
    assert_eq!(pins.len(), 23);
    for (i, (tag, fixed_hash)) in pins.iter().enumerate() {
        let values = SegmentPublicValues::decode(&expected_public_values(tag).unwrap()).unwrap();
        assert_eq!(usize::from(values.segment_index), i, "{tag}");
        assert_eq!(&values.fixed_hash, fixed_hash, "{tag}");
    }
    expected_public_values("B0L22").unwrap_err();
}

// --- What a proof file must be.

#[test]
fn a_file_that_is_no_compressed_proof_with_the_expected_values_is_rejected() {
    let expected = expected_public_values("B0L10").unwrap();
    for (name, bytes) in [
        ("empty", vec![]),
        ("text", b"an earlier proof".to_vec()),
        ("zeros", vec![0; 4096]),
    ] {
        let err = decode_segment_proof(&bytes, &expected).unwrap_err();
        assert!(
            err.contains("not a bincode SP1ProofWithPublicValues"),
            "{name}: {err}"
        );
    }
    // A well-formed proof file, but PLONK.
    let plonk = bincode::serialize(&plonk_proof([0x5a, 0x09, 0x3a, 0x2f], 960, &expected)).unwrap();
    let err = decode_segment_proof(&plonk, &expected).unwrap_err();
    assert!(err.contains("an SP1Proof::Plonk, not Compressed"), "{err}");
    // The same with a byte after it.
    let mut trailing = plonk.clone();
    trailing.push(0);
    let err = decode_segment_proof(&trailing, &expected).unwrap_err();
    assert!(
        err.contains("not a bincode SP1ProofWithPublicValues"),
        "{err}"
    );
}

#[test]
fn existing_proof_is_none_only_for_a_missing_path() {
    let dir = temp_dir("existing");
    let expected = expected_public_values("B0L10").unwrap();
    assert!(
        existing_proof(&dir.join("B0L10.proof.bin"), &expected)
            .unwrap()
            .is_none()
    );

    fs::write(dir.join("B0L10.proof.bin"), b"an earlier proof").unwrap();
    let err = existing_proof(&dir.join("B0L10.proof.bin"), &expected).unwrap_err();
    assert!(err.contains("is not a proof to resume from"), "{err}");
    assert!(err.contains("prove never overwrites"), "{err}");

    fs::create_dir(dir.join("head.proof.bin")).unwrap();
    std::os::unix::fs::symlink(dir.join("missing"), dir.join("B0L0.proof.bin")).unwrap();
    for tag in ["head", "B0L0"] {
        let err = existing_proof(&dir.join(format!("{tag}.proof.bin")), &expected).unwrap_err();
        assert!(err.contains("is not a regular file"), "{tag}: {err}");
    }
    fs::remove_dir_all(dir).unwrap();
}

// --- Out paths.

#[test]
fn out_dir_under_runs_or_outside_the_checkout_is_accepted() {
    let root = temp_dir("root-ok");
    fs::create_dir_all(root.join("runs/proofs")).unwrap();
    let outside = temp_dir("outside");
    let bundle = root.join("runs/tinyllama");
    for dir in [root.join("runs"), root.join("runs/proofs"), outside.clone()] {
        assert_eq!(check_out_dir(&dir, &root, &bundle).unwrap(), dir);
    }
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn out_dir_in_the_checkout_outside_runs_in_the_bundle_or_missing_is_rejected() {
    let root = temp_dir("root-bad");
    let bundle = root.join("runs/tinyllama");
    for dir in ["sp1", "runsx", "runs/tinyllama/proofs"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for (dir, why) in [
        (root.clone(), "not under its runs/"),
        (root.join("sp1"), "not under its runs/"),
        (root.join("runsx"), "not under its runs/"),
        // `..` resolves before the check.
        (root.join("runs/../sp1"), "not under its runs/"),
        (bundle.clone(), "in the bundle directory"),
        (bundle.join("proofs"), "in the bundle directory"),
        (root.join("runs/missing"), "No such file"),
    ] {
        let err = check_out_dir(&dir, &root, &bundle).unwrap_err();
        assert!(err.contains(why), "{}: {err}", dir.display());
    }
    // A bundle directory outside the checkout, and a file.
    let outside = temp_dir("bundle-outside");
    let err = check_out_dir(&outside, &root, &outside).unwrap_err();
    assert!(err.contains("in the bundle directory"), "{err}");
    fs::write(outside.join("file"), b"").unwrap();
    let err = check_out_dir(&outside.join("file"), &root, &bundle).unwrap_err();
    assert!(err.contains("not a directory"), "{err}");
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn new_file_must_be_new_and_under_runs_or_outside_the_checkout() {
    let root = temp_dir("root-file");
    fs::create_dir_all(root.join("runs")).unwrap();
    fs::create_dir_all(root.join("sp1")).unwrap();
    let outside = temp_dir("file-outside");
    for path in [root.join("runs/agg.bin"), outside.join("agg.bin")] {
        assert_eq!(check_new_file(&path, &root, "--out").unwrap(), path);
    }
    let err = check_new_file(&root.join("sp1/agg.bin"), &root, "--out").unwrap_err();
    assert!(err.contains("not under its runs/"), "{err}");
    check_new_file(&outside.join("missing/agg.bin"), &root, "--out").unwrap_err();

    // An existing file, and symlinks, dangling or not, never pass, there or anywhere.
    fs::write(outside.join("old.bin"), b"an earlier proof").unwrap();
    std::os::unix::fs::symlink(root.join("sp1/new.bin"), outside.join("dangling.bin")).unwrap();
    std::os::unix::fs::symlink(outside.join("old.bin"), outside.join("link.bin")).unwrap();
    for (name, why) in [
        ("old.bin", "already exists"),
        ("dangling.bin", "is a symlink"),
        ("link.bin", "is a symlink"),
    ] {
        let err = check_new_file(&outside.join(name), &root, "--out").unwrap_err();
        assert!(err.contains(why), "{name}: {err}");
        let err = check_new_path(&outside.join(name), "--fixture-out").unwrap_err();
        assert!(err.contains(why), "{name}: {err}");
    }
    // The fixture may land in the checkout (#80 commits it), but never over a file.
    assert_eq!(
        check_new_path(&root.join("sp1/fixture.json"), "--fixture-out").unwrap(),
        root.join("sp1/fixture.json")
    );
    assert!(!root.join("sp1/new.bin").exists());
    assert_eq!(
        fs::read(outside.join("old.bin")).unwrap(),
        b"an earlier proof"
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

/// What appeared at the checked path before the write: a file, a symlink to a pinned file and a
/// dangling symlink. `create_new` refuses each and leaves its target untouched.
#[test]
fn create_new_refuses_any_existing_path() {
    let dir = temp_dir("create-new");
    let pinned = dir.join("B0L10.proof.json");
    fs::write(&pinned, b"pinned").unwrap();
    fs::write(dir.join("file.proof.bin"), b"an earlier proof").unwrap();
    std::os::unix::fs::symlink(&pinned, dir.join("link.proof.bin")).unwrap();
    std::os::unix::fs::symlink(dir.join("missing"), dir.join("dangling.proof.bin")).unwrap();
    for name in ["file.proof.bin", "link.proof.bin", "dangling.proof.bin"] {
        create_new(&dir.join(name)).unwrap_err();
    }
    assert_eq!(fs::read(&pinned).unwrap(), b"pinned");
    assert!(!dir.join("missing").exists());
    create_new(&dir.join("B0L10.proof.bin")).unwrap();
    assert_eq!(fs::read(dir.join("B0L10.proof.bin")).unwrap(), b"");
    fs::remove_dir_all(dir).unwrap();
}

// --- aggregate's argument rules and its PLONK checks.

#[test]
fn aggregate_argument_rules() {
    let all = tinyllama_tags();
    let b0l10 = parse_tags("B0L10").unwrap();
    let twice = parse_tags("B0L10,B0L10").unwrap();
    // Accepted: the smoke run's steps, #80's wrap and its CI step.
    check_aggregate_args(&b0l10, Some(Mode::Compressed), false, false).unwrap();
    check_aggregate_args(&twice, Some(Mode::Compressed), false, false).unwrap();
    check_aggregate_args(&b0l10, Some(Mode::Plonk), false, false).unwrap();
    check_aggregate_args(&all, Some(Mode::Plonk), true, false).unwrap();
    check_aggregate_args(&all, None, false, true).unwrap();
    // Repeats only with --mode compressed.
    for mode in [Some(Mode::Plonk), None] {
        let err = check_aggregate_args(&twice, mode, false, true).unwrap_err();
        assert!(err.contains("only with --mode compressed"), "{err}");
    }
    // --fixture-out only with --mode plonk, without --execute-only, on the 23 tags in order.
    let mut reordered = all.clone();
    reordered.swap(0, 1);
    for (tags, mode, execute_only, why) in [
        (&all, Some(Mode::Compressed), false, "needs --mode plonk"),
        (
            &all,
            Some(Mode::Plonk),
            true,
            "needs --mode plonk and no --execute-only",
        ),
        (
            &b0l10,
            Some(Mode::Plonk),
            false,
            "the 23 TinyLlama tags in segment order",
        ),
        (
            &all[..22].to_vec(),
            Some(Mode::Plonk),
            false,
            "the 23 TinyLlama tags in segment order",
        ),
        (
            &reordered,
            Some(Mode::Plonk),
            false,
            "the 23 TinyLlama tags in segment order",
        ),
    ] {
        let err = check_aggregate_args(tags, mode, true, execute_only).unwrap_err();
        assert!(err.contains(why), "{err}");
    }
}

/// `proofBytes` must be 964 bytes starting `0x5a093a2f` (`SPEC.md` §1.2) and verify with
/// `sp1-verifier`: a wrong selector, a wrong length and a well-framed but forged proof are rejected.
#[test]
fn plonk_proof_bytes_off_the_spec_are_rejected() {
    let digest = [7; 32];
    let program_vkey = AGGREGATOR_GUEST
        .vkey
        .lines()
        .next()
        .unwrap()
        .strip_prefix("bytes32: ")
        .unwrap();
    for (selector, len, why) in [
        ([0x00, 0x09, 0x3a, 0x2f], 960, "not 964 starting 0x5a093a2f"),
        ([0x5a, 0x09, 0x3a, 0x2f], 959, "proofBytes are 963 bytes"),
        (
            [0x5a, 0x09, 0x3a, 0x2f],
            960,
            "PlonkVerifier rejects the proof",
        ),
    ] {
        let err =
            check_plonk_proof(&plonk_proof(selector, len, &digest), program_vkey).unwrap_err();
        assert!(err.contains(why), "{err}");
    }
}

#[test]
fn plonk_fixture_is_json_with_the_three_fields_in_order() {
    let json = plonk_fixture_json("0x00eb", &[0xcf, 0x52], &[0x5a, 0x09]);
    assert_eq!(
        json,
        "{\n  \"programVKey\": \"0x00eb\",\n  \"publicValues\": \"0xcf52\",\n  \"proofBytes\": \"0x5a09\"\n}\n"
    );
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["proofBytes"], "0x5a09");
}

// --- The commands refuse before any GPU work.

#[test]
fn prove_refuses_bad_tags_and_an_out_dir_in_the_checkout_before_reading_the_elf() {
    let dir = temp_dir("cmd-tags");
    let checkout = repo_root().join("sp1");
    for (tags, out_dir, why) in [
        ("B0L22", &dir, "unknown tag"),
        ("B0L10,B0L10", &dir, "names B0L10 twice"),
        ("B0L10", &checkout, "not under its runs/"),
    ] {
        let (code, stderr) = sp1_host(&[
            "prove",
            "--elf",
            "/nonexistent/segment-guest",
            "--bundle",
            "/nonexistent",
            "--tags",
            tags,
            "--out-dir",
            s(out_dir),
        ]);
        assert_eq!(code, 2, "{stderr}");
        assert!(stderr.contains(why), "{tags}: {stderr}");
    }
    assert!(!checkout.join("B0L10.proof.bin").exists());
    fs::remove_dir_all(dir).unwrap();
}

/// A `<tag>.proof.bin` that is not a proof to resume from refuses the whole run, before the ELF is
/// read, and stays as it was.
#[test]
fn prove_refuses_an_existing_non_proof_file_and_never_overwrites_it() {
    let dir = temp_dir("cmd-existing");
    fs::write(dir.join("head.proof.bin"), b"an earlier proof").unwrap();
    let (code, stderr) = sp1_host(&[
        "prove",
        "--elf",
        "/nonexistent/segment-guest",
        "--bundle",
        "/nonexistent",
        "--tags",
        "B0L10,head",
        "--out-dir",
        s(&dir),
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("head.proof.bin exists"), "{stderr}");
    assert!(stderr.contains("is not a proof to resume from"), "{stderr}");
    assert_eq!(
        fs::read(dir.join("head.proof.bin")).unwrap(),
        b"an earlier proof"
    );
    assert!(!dir.join("B0L10.proof.bin").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prove_refuses_an_elf_off_its_pin() {
    let dir = temp_dir("cmd-elf");
    let elf = dir.join("segment-guest");
    fs::write(&elf, b"not the segment guest").unwrap();
    let out = temp_dir("cmd-elf-out");
    let (code, stderr) = sp1_host(&[
        "prove",
        "--elf",
        s(&elf),
        "--bundle",
        "/nonexistent",
        "--tags",
        "B0L10",
        "--out-dir",
        s(&out),
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("sp1/segment-guest.elf.sha256"), "{stderr}");
    assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(out).unwrap();
}

#[test]
fn aggregate_refuses_bad_arguments_and_an_elf_off_its_pin() {
    let dir = temp_dir("cmd-aggregate");
    let elf = dir.join("guest");
    fs::write(&elf, b"not a guest").unwrap();
    let out = dir.join("agg.bin");
    let fixture = dir.join("fixture.json");
    let checkout_out = repo_root().join("sp1/agg.bin");
    let base = |tags: &'static str, mode: &'static str, out: &Path| {
        vec![
            "aggregate".to_string(),
            "--segment-elf".into(),
            s(&elf).into(),
            "--aggregator-elf".into(),
            s(&elf).into(),
            "--proofs".into(),
            s(&dir).into(),
            "--tags".into(),
            tags.into(),
            "--mode".into(),
            mode.into(),
            "--out".into(),
            s(out).into(),
        ]
    };
    let with_fixture = |mut args: Vec<String>| {
        args.extend(["--fixture-out".into(), s(&fixture).into()]);
        args
    };
    for (args, why) in [
        (base("B0L22", "compressed", &out), "unknown tag"),
        (
            base("B0L10,B0L10", "plonk", &out),
            "only with --mode compressed",
        ),
        (
            with_fixture(base("B0L10", "plonk", &out)),
            "the 23 TinyLlama tags in segment order",
        ),
        (
            with_fixture(base("all", "compressed", &out)),
            "needs --mode plonk",
        ),
        (
            base("B0L10", "compressed", &checkout_out),
            "not under its runs/",
        ),
        (
            base("B0L10", "compressed", &out),
            "sp1/segment-guest.elf.sha256",
        ),
    ] {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, stderr) = sp1_host(&args);
        assert_eq!(code, 2, "{stderr}");
        assert!(stderr.contains(why), "{args:?}: {stderr}");
    }
    // Without --execute-only, --mode and --out are required.
    let (code, stderr) = sp1_host(&[
        "aggregate",
        "--segment-elf",
        s(&elf),
        "--aggregator-elf",
        s(&elf),
        "--proofs",
        s(&dir),
        "--tags",
        "B0L10",
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("--mode") && stderr.contains("--out"),
        "{stderr}"
    );
    assert!(!out.exists() && !fixture.exists() && !checkout_out.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn aggregate_control_refuses_an_aggregator_elf_off_its_pin() {
    let dir = temp_dir("cmd-control");
    let elf = dir.join("guest");
    fs::write(&elf, b"not a guest").unwrap();
    let (code, stderr) = sp1_host(&[
        "aggregate-control",
        "--kind",
        "vk",
        "--aggregator-elf",
        s(&elf),
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("sp1/aggregator-guest.elf.sha256"),
        "{stderr}"
    );
    // B1 needs B0L10's proof and the segment guest.
    let (code, stderr) = sp1_host(&[
        "aggregate-control",
        "--kind",
        "pv-byte",
        "--aggregator-elf",
        s(&elf),
    ]);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("--segment-elf") && stderr.contains("--proofs"),
        "{stderr}"
    );
    fs::remove_dir_all(dir).unwrap();
}
