//! `sp1-host prove`'s checks that need no GPU (`docs/EVM-GAS-MODEL.md` §8): an ELF or vkey other than
//! the committed program's, a `fixedHash` other than its pin, and a proof path in the checkout's
//! tracked tree or in the bundle directory are each rejected; the command exits before it starts the
//! CUDA client.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sash_evm_primitives::segment::SegmentPublicValues;
use sp1_host::prove::{
    FIXED_PINS, VKEY, check_elf, check_fixed_hash, check_out, check_vkey, fixed_pin, repo_root,
};

/// `B0L10`'s line in `sp1/pins/tinyllama.fixed.sha256`.
const B0L10_FIXED_HASH: &str = "fa81937c933c38a91726b26260873a7e8edcbdc7cac92d908fd2ea2ee19faf47";

/// A fresh empty directory under the system's temp directory, outside the checkout.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sp1-host-prove-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn sp1_host_prove(elf: &Path, bundle: &Path, tag: &str, out: &Path) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_sp1-host"))
        .arg("prove")
        .arg("--elf")
        .arg(elf)
        .arg("--bundle")
        .arg(bundle)
        .args(["--tag", tag])
        .arg("--out")
        .arg(out)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn public_values(fixed_hash: [u8; 32]) -> Vec<u8> {
    SegmentPublicValues {
        segment_index: 10,
        pre_root: [1; 32],
        model_root: [2; 32],
        residual_in_root: [3; 32],
        residual_out_root: [4; 32],
        fixed_hash,
        tok_in_hash: [0; 32],
        tok_out_hash: [0; 32],
    }
    .encode()
    .to_vec()
}

#[test]
fn elf_with_another_sha256_is_rejected() {
    let err = check_elf(b"not the segment guest").unwrap_err();
    assert!(err.contains("sp1/segment-guest.elf.sha256"), "{err}");
}

#[test]
fn prove_rejects_another_elf_before_the_bundle_and_the_gpu() {
    let dir = temp_dir("elf");
    let elf = dir.join("segment-guest");
    fs::write(&elf, b"not the segment guest").unwrap();
    // No bundle exists: the ELF check comes first.
    let (code, stderr) = sp1_host_prove(
        &elf,
        &dir.join("no-bundle"),
        "B0L10",
        &dir.join("B0L10.proof.bin"),
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("sp1/segment-guest.elf.sha256"), "{stderr}");
    assert!(!dir.join("B0L10.proof.bin").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn committed_vkey_is_accepted() {
    check_vkey(VKEY).unwrap();
}

#[test]
fn another_vkey_is_rejected() {
    // One digit changed, at the last hex digit of `bytes32` or the last digit of `hash_u32[0]`.
    let bump = |at: usize| {
        let mut v = VKEY.as_bytes().to_vec();
        v[at] = if v[at] == b'0' { b'1' } else { b'0' };
        String::from_utf8(v).unwrap()
    };
    let other = bump(VKEY.find("0x").unwrap() + 2 + 63);
    assert_ne!(other, VKEY);
    let err = check_vkey(&other).unwrap_err();
    assert!(err.contains("sp1/segment-guest.vkey"), "{err}");
    let hash_u32 = bump(VKEY.find(',').unwrap() - 1);
    assert_ne!(hash_u32, VKEY);
    check_vkey(&hash_u32).unwrap_err();
}

#[test]
fn fixed_hash_equal_to_its_pin_is_accepted() {
    let pin = fixed_pin(FIXED_PINS, "B0L10").unwrap();
    assert_eq!(hex::encode(pin), B0L10_FIXED_HASH);
    assert_eq!(check_fixed_hash(&public_values(pin), &pin).unwrap(), pin);
}

#[test]
fn fixed_hash_other_than_its_pin_is_rejected() {
    let pin = fixed_pin(FIXED_PINS, "B0L10").unwrap();
    let mut other = pin;
    other[31] ^= 1;
    let err = check_fixed_hash(&public_values(other), &pin).unwrap_err();
    assert!(err.contains("differs from its pin"), "{err}");
    // Another segment's pin.
    let head = fixed_pin(FIXED_PINS, "head").unwrap();
    check_fixed_hash(&public_values(head), &pin).unwrap_err();
    // Public values of the wrong length.
    check_fixed_hash(&public_values(pin)[..225], &pin).unwrap_err();
}

#[test]
fn tag_without_a_pin_is_rejected() {
    fixed_pin(FIXED_PINS, "B0L22").unwrap_err();
}

#[test]
fn out_under_runs_or_outside_the_checkout_is_accepted() {
    let root = temp_dir("root-ok");
    fs::create_dir_all(root.join("runs/proofs")).unwrap();
    let outside = temp_dir("outside");
    assert_eq!(
        check_out(
            &root.join("runs/B0L10.proof.bin"),
            "B0L10",
            &root,
            &root.join("runs/tinyllama")
        )
        .unwrap(),
        root.join("runs/B0L10.proof.bin")
    );
    assert_eq!(
        check_out(
            &root.join("runs/proofs/head.proof.bin"),
            "head",
            &root,
            &root.join("runs/tinyllama")
        )
        .unwrap(),
        root.join("runs/proofs/head.proof.bin")
    );
    assert_eq!(
        check_out(
            &outside.join("head.proof.bin"),
            "head",
            &root,
            &root.join("runs/tinyllama")
        )
        .unwrap(),
        outside.join("head.proof.bin")
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn out_in_the_checkout_outside_runs_is_rejected() {
    let root = temp_dir("root-bad");
    fs::create_dir_all(root.join("sp1")).unwrap();
    fs::create_dir_all(root.join("runsx")).unwrap();
    for out in [
        "B0L10.proof.bin",
        "sp1/B0L10.proof.bin",
        "runsx/B0L10.proof.bin",
    ] {
        let err = check_out(
            &root.join(out),
            "B0L10",
            &root,
            &root.join("runs/tinyllama"),
        )
        .unwrap_err();
        assert!(err.contains("not under its runs/"), "{out}: {err}");
    }
    // `..` resolves before the check.
    fs::create_dir_all(root.join("runs")).unwrap();
    check_out(
        &root.join("runs/../sp1/B0L10.proof.bin"),
        "B0L10",
        &root,
        &root.join("runs/tinyllama"),
    )
    .unwrap_err();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn out_not_named_after_the_tag_or_in_no_directory_is_rejected() {
    let dir = temp_dir("name");
    let root = repo_root();
    for out in ["B0L11.proof.bin", "B0L10.proof", "proof.bin"] {
        let err =
            check_out(&dir.join(out), "B0L10", &root, &root.join("runs/tinyllama")).unwrap_err();
        assert!(
            err.contains("must be named B0L10.proof.bin"),
            "{out}: {err}"
        );
    }
    check_out(
        &dir.join("missing/B0L10.proof.bin"),
        "B0L10",
        &root,
        &root.join("runs/tinyllama"),
    )
    .unwrap_err();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prove_rejects_an_out_in_the_checkout_before_reading_the_elf() {
    let out = repo_root().join("sp1/B0L10.proof.bin");
    let (code, stderr) = sp1_host_prove(
        Path::new("/nonexistent/segment-guest"),
        Path::new("/nonexistent"),
        "B0L10",
        &out,
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("not under its runs/"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn out_in_the_bundle_directory_is_rejected() {
    let root = temp_dir("root-bundle");
    let bundle = root.join("runs/tinyllama");
    fs::create_dir_all(bundle.join("proofs")).unwrap();
    for out in [
        "runs/tinyllama/B0L10.proof.bin",
        "runs/tinyllama/proofs/B0L10.proof.bin",
    ] {
        let err = check_out(&root.join(out), "B0L10", &root, &bundle).unwrap_err();
        assert!(err.contains("in the bundle directory"), "{out}: {err}");
    }
    // Beside it under runs/, and a bundle directory outside the checkout.
    check_out(&root.join("runs/B0L10.proof.bin"), "B0L10", &root, &bundle).unwrap();
    let outside = temp_dir("bundle-outside");
    let err = check_out(&outside.join("B0L10.proof.bin"), "B0L10", &root, &outside).unwrap_err();
    assert!(err.contains("in the bundle directory"), "{err}");
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
