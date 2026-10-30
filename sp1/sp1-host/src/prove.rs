//! `sp1-host prove`: proves TinyLlama segments to compressed proofs on a local NVIDIA GPU, with one
//! CUDA client and one setup, resuming from the proofs already in `--out-dir` (`docs/EVM-GAS-MODEL.md`
//! §9). Every check that needs no GPU runs first; a failing tag does not stop the others. Also the out
//! path rules the proving commands share: proof bytes land under the checkout's `runs/` or outside the
//! checkout, never in the bundle directory, and no file is ever overwritten.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use sha2::{Digest, Sha256};
use sp1_sdk::blocking::prelude::*;
use sp1_sdk::blocking::{CudaProver, ProverClient};

use crate::SegmentInput;
use crate::gpu::with_cuda_client;
use crate::pins::{
    SEGMENT_GUEST, decode_segment_proof, expected_public_values, parse_tags, repeated_tag,
    repo_root, verify_on_cpu, vkey_text,
};

/// `dir` made absolute: an existing directory that is `<root>/runs/` or under it, or outside `root`,
/// and never `bundle` or inside it, once all three are canonicalized (so `../` and symlinked
/// directories resolve). Proof bytes never land in the checkout's tracked tree or beside the pinned
/// bundle files.
pub fn check_out_dir(dir: &Path, root: &Path, bundle: &Path) -> Result<PathBuf, String> {
    let dir = fs::canonicalize(dir).map_err(|e| format!("--out-dir {}: {e}", dir.display()))?;
    if !dir.is_dir() {
        return Err(format!("--out-dir {} is not a directory", dir.display()));
    }
    check_outside_tree(&dir, root, "--out-dir")?;
    // A bundle directory that does not exist cannot hold the existing `dir`.
    if let Ok(bundle) = fs::canonicalize(bundle)
        && dir.starts_with(&bundle)
    {
        return Err(format!(
            "--out-dir {} is in the bundle directory {}",
            dir.display(),
            bundle.display()
        ));
    }
    Ok(dir)
}

/// A new file's path, made absolute: not an existing path (writing follows a symlink, even a dangling
/// one, to a target the directory checks never see), in an existing directory that is `<root>/runs/`
/// or under it, or outside `root`. [`create_new`] refuses whatever appeared there meanwhile.
pub fn check_new_file(path: &Path, root: &Path, flag: &str) -> Result<PathBuf, String> {
    let path = check_new_path(path, flag)?;
    check_outside_tree(path.parent().expect("an absolute file path"), root, flag)?;
    Ok(path)
}

/// A new file's path, made absolute, anywhere: not an existing path, a symlink included, in an
/// existing directory.
pub fn check_new_path(path: &Path, flag: &str) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .ok_or(format!("{flag} {} names no file", path.display()))?;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let dir = fs::canonicalize(parent)
        .map_err(|e| format!("{flag}'s directory {}: {e}", parent.display()))?;
    let path = dir.join(name);
    match fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() => Err(format!(
            "{flag} {} is a symlink, which could lead into the checkout or the bundle directory",
            path.display()
        )),
        Ok(_) => Err(format!(
            "{flag} {} already exists; nothing is overwritten",
            path.display()
        )),
        Err(_) => Ok(path),
    }
}

/// Rejects a canonical `dir` inside the checkout `root` but not under its `runs/`.
fn check_outside_tree(dir: &Path, root: &Path, flag: &str) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    if dir.starts_with(&root) && !dir.starts_with(root.join("runs")) {
        return Err(format!(
            "{flag} {} is inside the checkout {} and not under its runs/",
            dir.display(),
            root.display()
        ));
    }
    Ok(())
}

/// Creates the file at `path`: `create_new`, so it refuses any existing path, a symlink or a dangling
/// one included, that appeared there since the check.
pub fn create_new(path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes `proof` to the new file `path` in `SP1ProofWithPublicValues::save`'s encoding (bincode),
/// through [`create_new`], and returns the bytes written.
pub fn write_proof_file(path: &Path, proof: &SP1ProofWithPublicValues) -> Result<Vec<u8>, String> {
    let bytes = bincode::serialize(proof).map_err(|e| e.to_string())?;
    io::Write::write_all(&mut create_new(path)?, &bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(bytes)
}

/// Prints a written file's path, size and sha256.
pub fn print_file(label: &str, path: &Path, bytes: &[u8]) {
    println!("{label}: {}", path.display());
    println!("{label} size: {} bytes", bytes.len());
    println!("{label} sha256: {}", hex::encode(Sha256::digest(bytes)));
}

/// The proof already at `path`, if any, decoded and with the expected public values. `Ok(None)` if
/// nothing is there; an error for anything else (a symlink, a directory, a file that is not such a
/// proof): `prove` never overwrites, so it refuses to start.
pub fn existing_proof(
    path: &Path,
    expected: &[u8],
) -> Result<Option<SP1ProofWithPublicValues>, String> {
    let refuse = |why: String| {
        format!(
            "{} exists and {why}; prove never overwrites: move it away",
            path.display()
        )
    };
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
        Ok(m) if !m.file_type().is_file() => Err(refuse("is not a regular file".into())),
        Ok(_) => {
            let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            decode_segment_proof(&bytes, expected)
                .map(Some)
                .map_err(|e| refuse(format!("is not a proof to resume from: {e}")))
        }
    }
}

/// `sp1-host prove`. On the CPU, before the CUDA client starts: the tags, the out directory, each
/// tag's existing `<tag>.proof.bin` (decoded, compared with its expected public values, then verified
/// against the segment vk: such a tag is skipped, anything else there refuses the run), the ELF's
/// sha256 and its vkey, and the bundle files of the tags left to prove. Then one CUDA client and one
/// setup, whose vkey is checked again, and each remaining tag in order.
pub fn prove(
    elf: &Path,
    bundle: &Path,
    tags: &str,
    out_dir: &Path,
    then: Option<&str>,
) -> Result<ExitCode, String> {
    let tags = parse_tags(tags)?;
    if let Some(tag) = repeated_tag(&tags) {
        return Err(format!("--tags names {tag} twice"));
    }
    let out_dir = check_out_dir(out_dir, &repo_root(), bundle)?;

    // Existing proofs are decoded and compared now, and verified once the vk is known.
    let mut existing = Vec::new();
    let mut todo = Vec::new();
    for tag in &tags {
        let path = out_dir.join(format!("{tag}.proof.bin"));
        match existing_proof(&path, &expected_public_values(tag)?)? {
            Some(proof) => existing.push((tag, proof)),
            None => todo.push((tag.clone(), path)),
        }
    }

    let light = ProverClient::builder().light().build();
    let (elf, vk) = SEGMENT_GUEST.load(elf, &light)?;
    for (tag, proof) in &existing {
        verify_on_cpu(&light, proof, &vk).map_err(|e| {
            format!(
                "{tag}.proof.bin in {} {e}; prove never overwrites: move it away",
                out_dir.display()
            )
        })?;
        println!("skip {tag}: {tag}.proof.bin verifies with the expected public values");
    }
    for (tag, _) in &todo {
        for kind in ["fixed", "claims", "proof"] {
            let path = bundle.join(format!("{tag}.{kind}.json"));
            if !path.is_file() {
                return Err(format!("{}: no such bundle file", path.display()));
            }
        }
    }
    if todo.is_empty() {
        println!("OK: every tag is already proved");
        return Ok(ExitCode::SUCCESS);
    }

    let failed = with_cuda_client(|client| {
        let t = Instant::now();
        let pk = client.setup(elf).map_err(|e| e.to_string())?;
        println!("setup time: {:.1} s", t.elapsed().as_secs_f64());
        SEGMENT_GUEST.check_vkey(&vkey_text(pk.verifying_key()))?;
        println!("OK: the GPU setup's vkey matches sp1/segment-guest.vkey");
        let mut failed = Vec::new();
        for (tag, path) in &todo {
            println!("== {tag}");
            match prove_tag(client, &pk, bundle, tag, path, then) {
                Ok(()) => println!("OK {tag}"),
                Err(e) => {
                    println!("FAILED {tag}: {e}");
                    failed.push(tag.as_str());
                }
            }
        }
        Ok(failed)
    })?;
    if failed.is_empty() {
        println!("OK: every tag proved and verified");
        Ok(ExitCode::SUCCESS)
    } else {
        println!("failed tags: {}", failed.join(","));
        Ok(ExitCode::FAILURE)
    }
}

/// One tag on the GPU: prove compressed, verify, compare the public values with the expected ones,
/// write `<tag>.proof.bin`, then run `then` on it.
fn prove_tag(
    client: &CudaProver,
    pk: &<CudaProver as Prover>::ProvingKey,
    bundle: &Path,
    tag: &str,
    path: &Path,
    then: Option<&str>,
) -> Result<(), String> {
    let input = SegmentInput::read_bundle(bundle, tag).map_err(|e| e.to_string())?;
    println!("stdin bytes: {}", input.len());
    let t = Instant::now();
    let proof = client
        .prove(pk, input.stdin())
        .compressed()
        .run()
        .map_err(|e| format!("proving failed: {e}"))?;
    println!("prove time: {:.1} s", t.elapsed().as_secs_f64());
    client
        .verify(&proof, pk.verifying_key(), None)
        .map_err(|e| format!("the proof does not verify: {e}"))?;
    let expected = expected_public_values(tag)?;
    if proof.public_values.as_slice() != expected {
        return Err(format!(
            "the proof's public values {} differ from the expected {}",
            hex::encode(proof.public_values.as_slice()),
            hex::encode(expected)
        ));
    }
    println!("OK: the proof verifies and its public values are the expected ones, byte for byte");
    let bytes = write_proof_file(path, &proof)?;
    print_file("proof", path, &bytes);
    if let Some(cmd) = then {
        run_then(cmd, path)?;
    }
    Ok(())
}

/// Runs `cmd` with `sh -c`, the proof's path as its `$1` (`--then 'upload-proof.sh "$1" segments'`).
fn run_then(cmd: &str, path: &Path) -> Result<(), String> {
    let t = Instant::now();
    let status = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .arg("sh")
        .arg(path)
        .status()
        .map_err(|e| format!("--then: {e}"))?;
    if !status.success() {
        return Err(format!("--then {cmd:?} exited with {status}"));
    }
    println!("--then: done in {:.1} s", t.elapsed().as_secs_f64());
    Ok(())
}
