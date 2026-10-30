//! The aggregator guest's host side (`SPEC.md` §1.2): its stdin, its executor run, the segment vkey
//! it hardcodes (read from its source so that a host test can compare it with
//! `sp1/segment-guest.vkey`), and the `aggregate` and `aggregate-control` commands, which prove it on
//! a local NVIDIA GPU over the segments' compressed proofs (`docs/EVM-GAS-MODEL.md` §9).

use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use clap::ValueEnum;
use sash_aggregate::aggregate;
use sash_evm_primitives::segment::SEGMENT_PUBLIC_VALUES_LEN;
use sash_evm_primitives::tinyllama_fixture::tinyllama_model_id;
use sp1_sdk::blocking::prelude::*;
use sp1_sdk::blocking::{CudaProver, ProverClient};
use sp1_sdk::{SP1Stdin, SP1VerifyingKey};
use sp1_verifier::{PLONK_VK_BYTES, PlonkVerifier};

use crate::gpu::with_cuda_client;
use crate::pins::{
    AGGREGATOR_GUEST, SEGMENT_GUEST, decode_segment_proof, expected_public_values, parse_tags,
    repeated_tag, repo_root, verify_on_cpu, vkey_text, write_proof,
};
use crate::prove::{check_new_file, check_new_path, create_new, print_file, write_proof_file};
use crate::{ECHO_GUEST_ELF, Execution, execute, run_executor, tinyllama_tags};

/// `SPEC.md` §1.4's `publicDigest`: what the aggregator commits on the 23 TinyLlama segments.
pub const TINYLLAMA_PUBLIC_DIGEST: &str =
    "cf52e19e82f82680f0098ab435b52e22512f3d634bd7a947d32abc76d067b3ef";

/// `proofBytes`' length (`SPEC.md` §1.2): the 4-byte selector, `exitCode`, `vkRoot`, `nonce` and the
/// 864-byte PLONK proof.
pub const PLONK_PROOF_BYTES_LEN: usize = 964;

/// `proofBytes[0..4]`: `VERIFIER_HASH()[0..4]` of the v6.1.0 `SP1VerifierPlonk` (`SPEC.md` §1.2).
pub const PLONK_SELECTOR: [u8; 4] = [0x5a, 0x09, 0x3a, 0x2f];

/// The byte binding control B1 flips in `B0L10`'s public values: inside `preRoot` (bytes `2..34`,
/// `SPEC.md` §1.1, §2.1).
pub const B1_FLIPPED_BYTE: usize = 10;

/// The aggregator's proof mode on the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Compressed,
    Plonk,
}

/// Which half of the `(vk, sha256(public values))` pair a binding control mutates (`SPEC.md` §2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ControlKind {
    /// B1: `B0L10`'s real proof, `segmentValues` with byte 10 flipped.
    PvByte,
    /// B2: the echo guest's proof of `B0L10`'s exact public values, under its own vk.
    Vk,
}

/// The aggregator guest's source, whose `SEGMENT_VKEY` literal [`segment_vkey`] reads.
pub const AGGREGATOR_GUEST_SOURCE: &str = include_str!("../../aggregator-guest/src/main.rs");

/// The aggregator's stdin without proofs: `modelId`, then `segmentValues`, each with `write_vec`.
pub fn aggregator_stdin(model_id: &[u8], segment_values: &[u8]) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(model_id.to_vec());
    stdin.write_vec(segment_values.to_vec());
    stdin
}

/// Runs the aggregator on `stdin` in the executor with deferred proof verification off, so that it
/// runs without the segment proofs: the executor treats `verify_sp1_proof` as a no-op either way
/// (`SPEC.md` §1.2).
pub fn execute_aggregator(elf: Elf, stdin: SP1Stdin) -> Result<Execution, String> {
    run_executor(elf, stdin, false)
}

/// The 23 TinyLlama `<tag>.public-values` in `dir` (as `sp1-host execute --public-values` writes
/// them), concatenated in segment order: the aggregator's `segmentValues`.
pub fn read_tinyllama_segment_values(dir: &Path) -> Result<Vec<u8>, String> {
    let mut values = Vec::with_capacity(23 * SEGMENT_PUBLIC_VALUES_LEN);
    for tag in tinyllama_tags() {
        let path = dir.join(format!("{tag}.public-values"));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() != SEGMENT_PUBLIC_VALUES_LEN {
            return Err(format!(
                "{}: {} bytes, not {SEGMENT_PUBLIC_VALUES_LEN}",
                path.display(),
                bytes.len()
            ));
        }
        values.extend_from_slice(&bytes);
    }
    Ok(values)
}

/// The `SEGMENT_VKEY` literal of the aggregator guest's `source`: the eight words between the
/// brackets after `const SEGMENT_VKEY: [u32; 8] = `.
pub fn segment_vkey(source: &str) -> Result<[u32; 8], String> {
    let decl = "const SEGMENT_VKEY: [u32; 8] = [";
    let start = source
        .find(decl)
        .ok_or(format!("no `{decl}` in the aggregator guest"))?
        + decl.len();
    let end = source[start..]
        .find(']')
        .ok_or("SEGMENT_VKEY has no closing bracket")?;
    parse_words(&source[start..start + end]).map_err(|e| format!("SEGMENT_VKEY: {e}"))
}

/// `hash_u32` of a vkey as `sp1-host vkey` prints it and `sp1/segment-guest.vkey` records it.
pub fn vkey_hash_u32(vkey: &str) -> Result<[u32; 8], String> {
    let words = vkey
        .lines()
        .find_map(|line| line.strip_prefix("hash_u32: ["))
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or("no `hash_u32: [...]` line")?;
    parse_words(words).map_err(|e| format!("hash_u32: {e}"))
}

/// Eight comma-separated `u32`s, with any whitespace and an optional trailing comma.
fn parse_words(list: &str) -> Result<[u32; 8], String> {
    let words = list
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(|w| w.parse::<u32>().map_err(|e| format!("{w:?}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    <[u32; 8]>::try_from(words).map_err(|w| format!("{} words, not 8", w.len()))
}

/// `aggregate`'s argument rules, before any file is read: tags repeat only with `--mode compressed`
/// (the `N = 2` timing run), and `--fixture-out` needs `--mode plonk`, no `--execute-only` and the 23
/// tags in segment order (the proof that settles on-chain).
pub fn check_aggregate_args(
    tags: &[String],
    mode: Option<Mode>,
    fixture_out: bool,
    execute_only: bool,
) -> Result<(), String> {
    if let Some(tag) = repeated_tag(tags)
        && mode != Some(Mode::Compressed)
    {
        return Err(format!(
            "--tags names {tag} twice: tags repeat only with --mode compressed"
        ));
    }
    if fixture_out {
        if mode != Some(Mode::Plonk) || execute_only {
            return Err("--fixture-out needs --mode plonk and no --execute-only".into());
        }
        if tags != tinyllama_tags() {
            return Err(
                "--fixture-out needs the 23 TinyLlama tags in segment order (--tags all)".into(),
            );
        }
    }
    Ok(())
}

/// What `aggregate`'s CPU checks hand to the GPU: the aggregator ELF and the stdin, with the
/// segment proofs, and the 32-byte digest it must commit.
struct Checked {
    aggregator_elf: Elf,
    stdin: SP1Stdin,
    digest: [u8; 32],
}

/// `aggregate`'s CPU checks, in order: both ELFs against their pins; the segment vk's `hash_u32`
/// against the aggregator's `SEGMENT_VKEY` (a host guard, `SPEC.md` §2.1); each tag's proof (decoded,
/// compressed, its public values the expected ones, verified against the segment vk); the digest
/// `aggregate` computes (`SPEC.md` §1.4's `publicDigest` on the 23 tags in order); and the aggregator
/// in the executor with the proofs in stdin, deferred proof verification on, committing that digest.
fn cpu_checks(
    segment_elf: &Path,
    aggregator_elf: &Path,
    proofs: &Path,
    tags: &[String],
) -> Result<Checked, String> {
    let light = ProverClient::builder().light().build();
    let (_, segment_vk) = SEGMENT_GUEST.load(segment_elf, &light)?;
    let (aggregator_elf, _) = AGGREGATOR_GUEST.load(aggregator_elf, &light)?;
    check_segment_vkey(&segment_vk)?;

    let mut checked: Vec<(&str, SP1ProofWithPublicValues)> = Vec::new();
    let mut values = Vec::with_capacity(tags.len() * SEGMENT_PUBLIC_VALUES_LEN);
    for tag in tags {
        let expected = expected_public_values(tag)?;
        values.extend_from_slice(&expected);
        if checked.iter().any(|(t, _)| t == tag) {
            continue;
        }
        let path = proofs.join(format!("{tag}.proof.bin"));
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let proof = decode_segment_proof(&bytes, &expected)
            .and_then(|proof| verify_on_cpu(&light, &proof, &segment_vk).map(|()| proof))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        checked.push((tag, proof));
    }
    println!(
        "OK: {} proof(s) decode to compressed proofs that verify on the CPU against the segment vk, with the expected public values",
        checked.len()
    );

    let model_id = tinyllama_model_id();
    let digest = aggregate(&model_id, &values)
        .map_err(|r| format!("aggregate rejects the public values: {r:?}"))?
        .public_digest;
    if tags == tinyllama_tags() {
        if hex::encode(digest) != TINYLLAMA_PUBLIC_DIGEST {
            return Err(format!(
                "aggregate gives {}, not SPEC.md §1.4's publicDigest {TINYLLAMA_PUBLIC_DIGEST}",
                hex::encode(digest)
            ));
        }
        println!("OK: aggregate gives SPEC.md §1.4's publicDigest {TINYLLAMA_PUBLIC_DIGEST}");
    }
    println!("digest (N = {}): {}", tags.len(), hex::encode(digest));

    let stdin = || -> Result<SP1Stdin, String> {
        let mut stdin = aggregator_stdin(&model_id, &values);
        for tag in tags {
            let (_, proof) = checked
                .iter()
                .find(|(t, _)| t == tag)
                .expect("checked above");
            write_proof(&mut stdin, proof, &segment_vk)?;
        }
        Ok(stdin)
    };
    let t = Instant::now();
    let run = execute(aggregator_elf.clone(), stdin()?)?;
    println!(
        "executor: exit code {}, {} cycles, {} PGU, {:.1} s",
        run.exit_code,
        run.cycles,
        run.gas,
        t.elapsed().as_secs_f64()
    );
    if run.exit_code != 0 {
        return Err(format!(
            "the aggregator exits {}; stderr:\n{}",
            run.exit_code, run.stderr
        ));
    }
    if run.public_values != digest {
        return Err(format!(
            "the aggregator commits {}, not the digest {}",
            hex::encode(&run.public_values),
            hex::encode(digest)
        ));
    }
    println!(
        "OK: the aggregator, with the proofs in stdin and deferred proof verification on, commits the digest"
    );
    Ok(Checked {
        aggregator_elf,
        stdin: stdin()?,
        digest,
    })
}

/// The host guard of `SPEC.md` §2.1: the segment vk's `hash_u32` is the aggregator's `SEGMENT_VKEY`.
/// It compares two pins and never reaches the recursion, so it is no binding control.
fn check_segment_vkey(segment_vk: &SP1VerifyingKey) -> Result<(), String> {
    let pinned = segment_vkey(AGGREGATOR_GUEST_SOURCE)?;
    if segment_vk.hash_u32() != pinned {
        return Err(format!(
            "the segment vk's hash_u32 {:?} is not the aggregator's SEGMENT_VKEY {pinned:?}",
            segment_vk.hash_u32()
        ));
    }
    println!("OK: the segment vk's hash_u32 is the aggregator's SEGMENT_VKEY");
    Ok(())
}

/// `sp1-host aggregate`'s arguments.
pub struct AggregateArgs<'a> {
    pub segment_elf: &'a Path,
    pub aggregator_elf: &'a Path,
    pub proofs: &'a Path,
    pub tags: &'a str,
    pub mode: Option<Mode>,
    pub out: Option<&'a Path>,
    pub fixture_out: Option<&'a Path>,
    pub execute_only: bool,
}

/// `sp1-host aggregate`: the argument rules and the out paths, [`cpu_checks`], then, unless
/// `--execute-only`, the aggregator proved on the GPU in `mode`, verified, its public values compared
/// with the digest (and, in PLONK mode, `proofBytes`' length and selector checked and the proof
/// verified with `sp1-verifier` too), and written to `out` (and the fixture to `fixture_out`).
pub fn aggregate_command(args: AggregateArgs) -> Result<ExitCode, String> {
    let tags = parse_tags(args.tags)?;
    check_aggregate_args(
        &tags,
        args.mode,
        args.fixture_out.is_some(),
        args.execute_only,
    )?;
    let out = match (args.execute_only, args.out) {
        (true, _) => None,
        (false, Some(out)) => Some(check_new_file(out, &repo_root(), "--out")?),
        (false, None) => return Err("--out is required without --execute-only".into()),
    };
    let fixture_out = args
        .fixture_out
        .map(|path| check_new_path(path, "--fixture-out"))
        .transpose()?;
    let checked = cpu_checks(args.segment_elf, args.aggregator_elf, args.proofs, &tags)?;
    let (Some(mode), Some(out)) = (args.mode, out) else {
        println!("OK: --execute-only: every CPU check passes");
        return Ok(ExitCode::SUCCESS);
    };

    let (proof, program_vkey) = with_cuda_client(|client| {
        let pk = setup_aggregator(client, checked.aggregator_elf)?;
        let t = Instant::now();
        let request = client.prove(&pk, checked.stdin);
        let proof = match mode {
            Mode::Compressed => request.compressed().run(),
            Mode::Plonk => request.plonk().run(),
        }
        .map_err(|e| format!("proving failed: {e}"))?;
        println!("prove time ({mode:?}): {:.1} s", t.elapsed().as_secs_f64());
        client
            .verify(&proof, pk.verifying_key(), None)
            .map_err(|e| format!("the proof does not verify: {e}"))?;
        println!("OK: the proof verifies with the SDK");
        Ok((proof, pk.verifying_key().bytes32()))
    })?;
    if proof.public_values.as_slice() != checked.digest {
        return Err(format!(
            "the proof's public values {} are not the digest {}",
            hex::encode(proof.public_values.as_slice()),
            hex::encode(checked.digest)
        ));
    }
    println!("OK: its public values are the 32-byte digest");
    let proof_bytes = match mode {
        Mode::Compressed => None,
        Mode::Plonk => Some(check_plonk_proof(&proof, &program_vkey)?),
    };

    let bytes = write_proof_file(&out, &proof)?;
    print_file("proof", &out, &bytes);
    if let Some(proof_bytes) = proof_bytes {
        let json = plonk_fixture_json(&program_vkey, &checked.digest, &proof_bytes);
        print!("fixture:\n{json}");
        if let Some(path) = fixture_out {
            std::io::Write::write_all(&mut create_new(&path)?, json.as_bytes())
                .map_err(|e| format!("{}: {e}", path.display()))?;
            println!("fixture written: {}", path.display());
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The aggregator's GPU setup, timed, its vkey checked against `sp1/aggregator-guest.vkey`.
fn setup_aggregator(
    client: &CudaProver,
    elf: Elf,
) -> Result<<CudaProver as Prover>::ProvingKey, String> {
    let t = Instant::now();
    let pk = client.setup(elf).map_err(|e| e.to_string())?;
    println!("aggregator setup time: {:.1} s", t.elapsed().as_secs_f64());
    AGGREGATOR_GUEST.check_vkey(&vkey_text(pk.verifying_key()))?;
    println!("OK: the GPU setup's vkey matches sp1/aggregator-guest.vkey");
    Ok(pk)
}

/// A PLONK proof's `proofBytes` (`SP1ProofWithPublicValues::bytes`), if they are 964 bytes starting
/// with the v6.1.0 selector and `sp1-verifier`'s `PlonkVerifier` accepts them with its embedded
/// `PLONK_VK_BYTES` (`SPEC.md` §1.2).
pub fn check_plonk_proof(
    proof: &SP1ProofWithPublicValues,
    program_vkey: &str,
) -> Result<Vec<u8>, String> {
    if proof.tee_proof.is_some() {
        return Err("the proof carries a TEE proof, which would move the selector".into());
    }
    let bytes = proof.bytes();
    if bytes.len() != PLONK_PROOF_BYTES_LEN || bytes[..4] != PLONK_SELECTOR {
        return Err(format!(
            "proofBytes are {} bytes starting 0x{}, not {PLONK_PROOF_BYTES_LEN} starting 0x{}",
            bytes.len(),
            hex::encode(&bytes[..bytes.len().min(4)]),
            hex::encode(PLONK_SELECTOR)
        ));
    }
    PlonkVerifier::verify(
        &bytes,
        proof.public_values.as_slice(),
        program_vkey,
        &PLONK_VK_BYTES,
    )
    .map_err(|e| format!("sp1-verifier's PlonkVerifier rejects the proof: {e}"))?;
    println!(
        "OK: proofBytes are {PLONK_PROOF_BYTES_LEN} bytes with selector 0x{}, and sp1-verifier's PlonkVerifier accepts them",
        hex::encode(PLONK_SELECTOR)
    );
    Ok(bytes)
}

/// The PLONK fixture (`contracts/fixtures/tinyllama.plonk.json`): `programVKey`, `publicValues` and
/// `proofBytes`, as 0x-prefixed hex, in that order.
pub fn plonk_fixture_json(program_vkey: &str, public_values: &[u8], proof_bytes: &[u8]) -> String {
    format!(
        "{{\n  \"programVKey\": \"{program_vkey}\",\n  \"publicValues\": \"0x{}\",\n  \"proofBytes\": \"0x{}\"\n}}\n",
        hex::encode(public_values),
        hex::encode(proof_bytes)
    )
}

/// `sp1-host aggregate-control`: binding control B1 or B2 (`SPEC.md` §2.1) at `N = 1` on `B0L10`,
/// with `aggregate`'s pre-checks skipped. Only the control's own half of the `(vk, sha256(public
/// values))` pair differs from the honest run: the other half is checked first (B1: `B0L10`'s proof
/// verifies with its expected values; B2: the echo proof verifies under its own vk and carries
/// `B0L10`'s exact public values). The aggregator is then proved in compressed mode, where the
/// deferred-proof recursion runs. Exit 0 (pass) only if proving fails or the proof does not verify;
/// exit 1 on a verifying proof. It sets no time limit: `gpu-prove.sh` caps it and reports a timeout as
/// inconclusive.
pub fn aggregate_control(
    kind: ControlKind,
    segment_elf: Option<&Path>,
    aggregator_elf: &Path,
    proofs: Option<&Path>,
) -> Result<ExitCode, String> {
    let light = ProverClient::builder().light().build();
    let (aggregator_elf, _) = AGGREGATOR_GUEST.load(aggregator_elf, &light)?;
    let model_id = tinyllama_model_id();
    let b0l10 = expected_public_values("B0L10")?;
    let passed = match kind {
        ControlKind::PvByte => {
            let (segment_elf, proofs) = segment_elf
                .zip(proofs)
                .ok_or("--kind pv-byte needs --segment-elf and --proofs")?;
            let (_, segment_vk) = SEGMENT_GUEST.load(segment_elf, &light)?;
            let path = proofs.join("B0L10.proof.bin");
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let proof = decode_segment_proof(&bytes, &b0l10)
                .and_then(|proof| verify_on_cpu(&light, &proof, &segment_vk).map(|()| proof))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            println!(
                "honest half: B0L10's proof verifies on the CPU with its expected public values"
            );
            let mut values = b0l10;
            values[B1_FLIPPED_BYTE] ^= 0xff;
            println!(
                "B1: segmentValues are B0L10's with byte {B1_FLIPPED_BYTE} (in preRoot) flipped: 0x{:02x} -> 0x{:02x}",
                b0l10[B1_FLIPPED_BYTE], values[B1_FLIPPED_BYTE]
            );
            let mut stdin = aggregator_stdin(&model_id, &values);
            write_proof(&mut stdin, &proof, &segment_vk)?;
            with_cuda_client(|client| prove_control(client, aggregator_elf, stdin))?
        }
        ControlKind::Vk => with_cuda_client(|client| {
            let t = Instant::now();
            let echo_pk = client.setup(ECHO_GUEST_ELF).map_err(|e| e.to_string())?;
            let echo_vk = echo_pk.verifying_key();
            println!("echo setup time: {:.1} s", t.elapsed().as_secs_f64());
            let segment_vkey = segment_vkey(AGGREGATOR_GUEST_SOURCE)?;
            if echo_vk.hash_u32() == segment_vkey {
                return Err("the echo guest's hash_u32 is SEGMENT_VKEY".into());
            }
            println!(
                "echo guest hash_u32: {:?} (not SEGMENT_VKEY)",
                echo_vk.hash_u32()
            );
            let mut stdin = SP1Stdin::new();
            stdin.write_vec(b0l10.to_vec());
            let t = Instant::now();
            let echo = client
                .prove(&echo_pk, stdin)
                .compressed()
                .run()
                .map_err(|e| format!("proving the echo guest failed: {e}"))?;
            println!("echo prove time: {:.1} s", t.elapsed().as_secs_f64());
            client
                .verify(&echo, echo_vk, None)
                .map_err(|e| format!("the echo proof does not verify: {e}"))?;
            if echo.public_values.as_slice() != b0l10 {
                return Err("the echo proof's public values are not B0L10's".into());
            }
            println!(
                "honest half: the echo proof verifies under its own vk and carries B0L10's exact public values"
            );
            let mut stdin = aggregator_stdin(&model_id, &b0l10);
            write_proof(&mut stdin, &echo, echo_vk)?;
            prove_control(client, aggregator_elf, stdin)
        })?,
    };
    Ok(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Proves the aggregator on `stdin` in compressed mode and reports the control's outcome: `true`
/// (pass) if proving fails or the proof does not verify.
fn prove_control(client: &CudaProver, elf: Elf, stdin: SP1Stdin) -> Result<bool, String> {
    let pk = setup_aggregator(client, elf)?;
    let t = Instant::now();
    let proved = client.prove(&pk, stdin).compressed().run();
    let secs = t.elapsed().as_secs_f64();
    Ok(match proved {
        Err(e) => {
            println!("PASS: proving fails after {secs:.1} s: {e}");
            true
        }
        Ok(proof) => match client.verify(&proof, pk.verifying_key(), None) {
            Err(e) => {
                println!("PASS: proved in {secs:.1} s, and the proof does not verify: {e}");
                true
            }
            Ok(()) => {
                println!(
                    "FAIL: proved in {secs:.1} s, and the proof verifies, public values {}: the binding is not enforced",
                    hex::encode(proof.public_values.as_slice())
                );
                false
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_literal_and_the_pin_format() {
        let source =
            "/// x\nconst SEGMENT_VKEY: [u32; 8] = [\n    1, 2, 3,\n    4, 5, 6, 7, 8,\n];\n";
        assert_eq!(segment_vkey(source), Ok([1, 2, 3, 4, 5, 6, 7, 8]));
        let pin = "bytes32: 0x00\nhash_u32: [1, 2, 3, 4, 5, 6, 7, 4294967295]\n";
        assert_eq!(vkey_hash_u32(pin), Ok([1, 2, 3, 4, 5, 6, 7, u32::MAX]));
    }

    #[test]
    fn rejects_another_word_count_or_a_non_number() {
        assert!(segment_vkey("const SEGMENT_VKEY: [u32; 8] = [1, 2, 3, 4, 5, 6, 7];").is_err());
        assert!(vkey_hash_u32("hash_u32: [1, 2, 3, 4, 5, 6, 7, 8, 9]").is_err());
        assert!(vkey_hash_u32("hash_u32: [1, 2, 3, 4, 5, 6, 7, -8]").is_err());
        assert!(vkey_hash_u32("bytes32: 0x00\n").is_err());
    }
}
