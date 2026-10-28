//! `sp1-host`: runs the segment guest on one bundle in the SP1 executor, prints a guest's vkey, runs
//! the controls on the real TinyLlama bundles, prints their proof sizes, or proves one segment on a
//! GPU.
//!
//! - `sp1-host execute [--elf PATH] --bundle DIR --tag T [--public-values PATH]`
//! - `sp1-host vkey --elf PATH`
//! - `sp1-host prove --elf PATH --bundle DIR --tag T --out FILE`
//! - `sp1-host controls --elf PATH --bundles DIR --out DIR --pins FILE`
//! - `sp1-host sizes --bundles DIR`
//! - `sp1-host trace-share --trace FILE [--function NAME]`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use sp1_host::controls::CONTROLS;
use sp1_host::prove::{
    FIXED_PINS, check_elf, check_fixed_hash, check_out, check_vkey, fixed_pin, gpu_server_vm_hwm,
    repo_root, vkey_text,
};
use sp1_host::sizes::{Sizes, table};
use sp1_host::trace::function_samples;
use sp1_host::{
    CYCLE_LIMIT, SEGMENT_GUEST_ELF, SegmentInput, execute, is_cycle_limit, peak_rss,
    read_fixed_pins, tinyllama_tags,
};
use sp1_sdk::blocking::{ProverClient, prelude::*};

#[derive(Parser)]
#[command(about = "Host of the SP1 segment guest (SPEC.md §1.1)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Runs the segment guest on `<tag>.{fixed,claims,proof}.json` in the executor.
    Execute {
        /// The guest ELF to run instead of the one `build.rs` built (e.g. the Docker build).
        #[arg(long)]
        elf: Option<PathBuf>,
        /// The bundle directory.
        #[arg(long)]
        bundle: PathBuf,
        /// The segment's tag.
        #[arg(long)]
        tag: String,
        /// On exit code 0, writes the 226 committed bytes here verbatim.
        #[arg(long)]
        public_values: Option<PathBuf>,
    },
    /// Prints a guest's vkey, `vk.bytes32()` and `vk.hash_u32()`, from `LightProver::setup`.
    Vkey {
        #[arg(long)]
        elf: PathBuf,
    },
    /// Proves `<tag>.{fixed,claims,proof}.json` on the local NVIDIA GPU, in compressed mode, with #43's
    /// Docker-built ELF; verifies the proof and checks its public values against an executor run on
    /// the same input and its `fixedHash` against its pin (`docs/EVM-GAS-MODEL.md` §8). Linux x86_64.
    Prove {
        /// #43's Docker-built segment guest: its sha256 and vkey must be the committed ones.
        #[arg(long)]
        elf: PathBuf,
        /// The bundle directory.
        #[arg(long)]
        bundle: PathBuf,
        /// The segment's tag.
        #[arg(long)]
        tag: String,
        /// Where the compressed proof goes: `<tag>.proof.bin`, under the checkout's `runs/` or outside
        /// the checkout.
        #[arg(long)]
        out: PathBuf,
    },
    /// Runs the controls on modified copies of the TinyLlama bundles, written under
    /// `<out>/controls/`, after `make sp1-execute` has written `<out>/<tag>.public-values`.
    Controls {
        #[arg(long)]
        elf: PathBuf,
        #[arg(long)]
        bundles: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// `sp1/pins/tinyllama.fixed.sha256`.
        #[arg(long)]
        pins: PathBuf,
    },
    /// Prints `docs/EVM-GAS-MODEL.md` §1's proof sizes, recomputed from the TinyLlama bundles.
    Sizes {
        #[arg(long)]
        bundles: PathBuf,
    },
    /// Prints the share of a profiler trace's samples (`TRACE_FILE`) under one function.
    TraceShare {
        #[arg(long)]
        trace: PathBuf,
        /// A substring of the function's demangled name, which carries its crate's disambiguator
        /// (`<blake2[…]::Blake2sVarCore>::compress`).
        #[arg(long, default_value = "::Blake2sVarCore>::compress")]
        function: String,
    },
}

fn read_elf(path: &PathBuf) -> Result<Elf, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Elf::from(bytes))
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Command::Execute {
            elf,
            bundle,
            tag,
            public_values,
        } => {
            let elf = match elf {
                Some(path) => read_elf(&path)?,
                None => SEGMENT_GUEST_ELF,
            };
            let input = SegmentInput::read_bundle(&bundle, &tag).map_err(|e| e.to_string())?;
            let run = match execute(elf, input.stdin()) {
                Err(e) if is_cycle_limit(&e) => {
                    println!("cycle limit: exceeded CYCLE_LIMIT = {CYCLE_LIMIT} (not a rejection)");
                    println!("stdin bytes: {}", input.len());
                    return Ok(ExitCode::from(3));
                }
                run => run?,
            };
            println!("exit code: {}", run.exit_code);
            println!("public values: {}", hex::encode(&run.public_values));
            println!("cycles: {}", run.cycles);
            println!("PGU: {}", run.gas);
            println!("stdin bytes: {}", input.len());
            println!("peak RSS: {} bytes", run.peak_rss);
            if run.exit_code != 0 {
                println!("stderr:\n{}", run.stderr);
                return Ok(ExitCode::FAILURE);
            }
            if let Some(path) = public_values {
                fs::write(&path, &run.public_values)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Vkey { elf } => {
            let client = ProverClient::builder().light().build();
            let pk = client.setup(read_elf(&elf)?).map_err(|e| e.to_string())?;
            print!("{}", vkey_text(pk.verifying_key()));
            Ok(ExitCode::SUCCESS)
        }
        Command::Prove {
            elf,
            bundle,
            tag,
            out,
        } => prove(&elf, &bundle, &tag, &out),
        Command::Controls {
            elf,
            bundles,
            out,
            pins,
        } => {
            let elf = read_elf(&elf)?;
            let pins = read_fixed_pins(&pins).map_err(|e| e.to_string())?;
            for control in &CONTROLS {
                let pin = &pins
                    .iter()
                    .find(|(tag, _)| tag == control.tag)
                    .ok_or(format!("no pin for {}", control.tag))?
                    .1;
                let original = read(&out.join(format!("{}.public-values", control.tag)))?;
                let dir = out.join("controls").join(control.name);
                control.write_copy(&bundles, &dir)?;
                let (run, result) = control.run(elf.clone(), &dir, &original, pin)?;
                println!(
                    "{} ({} {}: {}): {result}; {} cycles",
                    control.name,
                    control.tag,
                    control.file.kind(),
                    control.what,
                    run.cycles
                );
            }
            println!("OK: every control has its expected outcome");
            Ok(ExitCode::SUCCESS)
        }
        Command::Sizes { bundles } => {
            let all = tinyllama_tags()
                .into_iter()
                .map(|tag| {
                    let sizes = Sizes::of_bundle(&bundles, &tag).map_err(|e| e.to_string())?;
                    Ok((tag, sizes))
                })
                .collect::<Result<Vec<_>, String>>()?;
            print!("{}", table(&all, &["B0L0", "B0L10", "head"]));
            Ok(ExitCode::SUCCESS)
        }
        Command::TraceShare { trace, function } => {
            let trace: serde_json::Value =
                serde_json::from_slice(&read(&trace)?).map_err(|e| e.to_string())?;
            let (samples, matched) = function_samples(&trace, &function)?;
            println!("samples: {samples}");
            println!("under {function}: {matched}");
            println!("share: {:.4}", matched as f64 / samples as f64);
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `sp1-host prove`. Every check that needs no GPU runs before the CUDA client starts.
fn prove(elf: &Path, bundle: &Path, tag: &str, out: &Path) -> Result<ExitCode, String> {
    let out = check_out(out, tag, &repo_root())?;
    let elf_bytes = read(elf)?;
    check_elf(&elf_bytes)?;
    println!("OK: the ELF matches sp1/segment-guest.elf.sha256");
    let pin = fixed_pin(FIXED_PINS, tag)?;
    let elf = Elf::from(elf_bytes);
    let input = SegmentInput::read_bundle(bundle, tag).map_err(|e| e.to_string())?;
    println!("stdin bytes: {}", input.len());

    // The executor run, on the CPU: the reference for the proof's public values.
    let t = Instant::now();
    let run = execute(elf.clone(), input.stdin())?;
    println!("execute time: {:.1} s", t.elapsed().as_secs_f64());
    println!("exit code: {}", run.exit_code);
    println!("cycles: {}", run.cycles);
    println!("PGU: {}", run.gas);
    // On Linux x86_64 the native executor runs the guest in a child process, which this counts.
    println!("executor peak RSS: {} bytes", run.peak_rss);
    if run.exit_code != 0 {
        return Err(format!(
            "the executor rejects {tag}; stderr:\n{}",
            run.stderr
        ));
    }

    // Building the client downloads `sp1-gpu-server` on first use and starts it: the warm-up, timed
    // apart from setup and proving.
    let t = Instant::now();
    let client = ProverClient::builder().cuda().build();
    println!(
        "client build time (server download and start): {:.1} s",
        t.elapsed().as_secs_f64()
    );

    let t = Instant::now();
    let pk = client.setup(elf).map_err(|e| e.to_string())?;
    println!("setup time: {:.1} s", t.elapsed().as_secs_f64());
    check_vkey(&vkey_text(pk.verifying_key()))?;
    println!("OK: the vkey matches sp1/segment-guest.vkey");

    let t = Instant::now();
    let proof = client
        .prove(&pk, input.stdin())
        .compressed()
        .run()
        .map_err(|e| e.to_string())?;
    println!("prove time: {:.1} s", t.elapsed().as_secs_f64());
    match gpu_server_vm_hwm() {
        Some(hwm) => println!("sp1-gpu-server VmHWM: {hwm}"),
        None => println!("sp1-gpu-server VmHWM: not found"),
    }

    let t = Instant::now();
    client
        .verify(&proof, pk.verifying_key(), None)
        .map_err(|e| format!("the proof does not verify: {e}"))?;
    println!("verify time: {:.1} s", t.elapsed().as_secs_f64());
    if proof.public_values.as_slice() != run.public_values.as_slice() {
        return Err(format!(
            "the proof's public values {} differ from the executor's {}",
            hex::encode(proof.public_values.as_slice()),
            hex::encode(&run.public_values)
        ));
    }
    println!("public values: {}", hex::encode(&run.public_values));
    println!("OK: the proof verifies and its public values equal the executor's, byte for byte");
    let fixed_hash = check_fixed_hash(proof.public_values.as_slice(), &pin)?;
    println!(
        "OK: fixedHash {} equals its pin in sp1/pins/tinyllama.fixed.sha256",
        hex::encode(fixed_hash)
    );

    proof
        .save(&out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    let bytes = read(&out)?;
    println!("proof: {}", out.display());
    println!("proof size: {} bytes", bytes.len());
    println!("proof sha256: {}", hex::encode(Sha256::digest(&bytes)));
    println!("host peak RSS: {} bytes", peak_rss());
    Ok(ExitCode::SUCCESS)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> ExitCode {
    run(Cli::parse()).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    })
}
