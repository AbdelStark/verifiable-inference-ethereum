//! `sp1-host`: runs the segment guest on one bundle in the SP1 executor, prints a guest's vkey, runs
//! the controls on the real TinyLlama bundles, or prints their proof sizes.
//!
//! - `sp1-host execute [--elf PATH] --bundle DIR --tag T [--public-values PATH]`
//! - `sp1-host vkey --elf PATH`
//! - `sp1-host controls --elf PATH --bundles DIR --out DIR --pins FILE`
//! - `sp1-host sizes --bundles DIR`
//! - `sp1-host trace-share --trace FILE [--function NAME]`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sp1_host::controls::CONTROLS;
use sp1_host::sizes::{Sizes, table};
use sp1_host::trace::function_samples;
use sp1_host::{
    CYCLE_LIMIT, SEGMENT_GUEST_ELF, SegmentInput, execute, is_cycle_limit, read_fixed_pins,
    tinyllama_tags,
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
            let vk = pk.verifying_key();
            println!("bytes32: {}", vk.bytes32());
            println!("hash_u32: {:?}", vk.hash_u32());
            Ok(ExitCode::SUCCESS)
        }
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

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> ExitCode {
    run(Cli::parse()).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    })
}
