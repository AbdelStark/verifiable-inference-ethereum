//! `sp1-host`: runs the segment guest on one bundle in the SP1 executor, runs the aggregator guest on
//! the 23 segments' public values, prints a guest's vkey, proves segments, aggregates their proofs and
//! runs the binding controls on a GPU, runs the controls on the real TinyLlama bundles, prints their
//! proof sizes, or renders `TinyLlamaFixture.sol` from the 23 segments' public values.
//!
//! - `sp1-host execute [--elf PATH] --bundle DIR --tag T [--public-values PATH]`
//! - `sp1-host execute-aggregator [--elf PATH] --public-values DIR`
//! - `sp1-host vkey --elf PATH`
//! - `sp1-host prove --elf PATH --bundle DIR --tags B0L10,head|all --out-dir DIR [--then CMD]`
//! - `sp1-host aggregate --segment-elf PATH --aggregator-elf PATH --proofs DIR --tags …|all
//!   [--mode compressed|plonk --out FILE] [--fixture-out FILE] [--execute-only]`
//! - `sp1-host aggregate-control --kind pv-byte|vk --aggregator-elf PATH [--segment-elf PATH --proofs DIR]`
//! - `sp1-host controls --elf PATH --bundles DIR --out DIR --pins FILE`
//! - `sp1-host sizes --bundles DIR`
//! - `sp1-host trace-share --trace FILE [--function NAME]`
//! - `sp1-host fixture-from-bundles --public-values DIR --bundles DIR --prompt-len L --out PATH`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sash_aggregate::aggregate;
use sash_evm_primitives::tinyllama_fixture::tinyllama_model_id;
use sp1_host::aggregate::{
    AggregateArgs, ControlKind, Mode, aggregate_command, aggregate_control, aggregator_stdin,
    execute_aggregator, read_tinyllama_segment_values,
};
use sp1_host::controls::CONTROLS;
use sp1_host::fixture::{check_fixture_out, contracts_src, fixture_from_bundles};
use sp1_host::pins::vkey_text;
use sp1_host::prove::prove;
use sp1_host::sizes::{Sizes, table};
use sp1_host::trace::function_samples;
use sp1_host::{
    AGGREGATOR_GUEST_ELF, CYCLE_LIMIT, SEGMENT_GUEST_ELF, SegmentInput, execute, is_cycle_limit,
    read_fixed_pins, tinyllama_tags,
};
use sp1_sdk::blocking::{ProverClient, prelude::*};

#[derive(Parser)]
#[command(about = "Host of the SP1 segment and aggregator guests (SPEC.md §1.1, §1.2)")]
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
    /// Runs the aggregator guest in the executor, without the segment proofs (deferred proof
    /// verification off), on the TinyLlama `modelId` and the 23 segments' `<tag>.public-values` (as
    /// `execute --public-values` writes them), and checks that it commits what `sash-aggregate`'s
    /// `aggregate` computes natively (`SPEC.md` §1.2).
    ExecuteAggregator {
        /// The guest ELF to run instead of the one `build.rs` built (e.g. the Docker build).
        #[arg(long)]
        elf: Option<PathBuf>,
        /// The directory holding `<tag>.public-values` for `B0L0..B0L21` and `head`.
        #[arg(long)]
        public_values: PathBuf,
    },
    /// Prints a guest's vkey, `vk.bytes32()` and `vk.hash_u32()`, from `LightProver::setup`.
    Vkey {
        #[arg(long)]
        elf: PathBuf,
    },
    /// Proves TinyLlama segments to compressed proofs on the local NVIDIA GPU with one CUDA client and
    /// one setup, from the Docker-built segment guest (`docs/EVM-GAS-MODEL.md` §9). Linux x86_64.
    /// Checks first, on the CPU: the out directory, each `<tag>.proof.bin` already there (a verified
    /// proof with the expected public values is skipped; anything else refuses the run), the ELF's
    /// sha256 and vkey. Each proof must verify and carry the expected public values
    /// (`tinyllama_segment_public_values`) byte for byte; it is written as `<tag>.proof.bin`, then
    /// `--then` runs on it. A failing tag does not stop the others; the exit code is 1 if any failed.
    Prove {
        /// The Docker-built segment guest: its sha256 and vkey must be the committed ones.
        #[arg(long)]
        elf: PathBuf,
        /// The bundle directory.
        #[arg(long)]
        bundle: PathBuf,
        /// Comma-separated TinyLlama tags, proved in this order, or `all`.
        #[arg(long)]
        tags: String,
        /// Where `<tag>.proof.bin` go: an existing directory under the checkout's `runs/` or outside
        /// the checkout, never in the bundle directory.
        #[arg(long)]
        out_dir: PathBuf,
        /// A shell command run with `sh -c` after each proof is written, the proof's path as its `$1`
        /// (e.g. `'sp1/scripts/upload-proof.sh "$1" segments'`); a non-zero exit fails the tag.
        #[arg(long)]
        then: Option<String>,
    },
    /// Aggregates segment proofs: every CPU check first (both ELFs against their pins, the segment vk
    /// against `SEGMENT_VKEY`, each proof verified with the expected public values, the digest, and the
    /// aggregator in the executor over the proofs), then proves the aggregator on the local NVIDIA GPU
    /// in `--mode`, verifies it and checks its public values (`SPEC.md` §1.2, `docs/EVM-GAS-MODEL.md`
    /// §9).
    Aggregate {
        /// The Docker-built segment guest.
        #[arg(long)]
        segment_elf: PathBuf,
        /// The Docker-built aggregator guest.
        #[arg(long)]
        aggregator_elf: PathBuf,
        /// The directory holding `<tag>.proof.bin`.
        #[arg(long)]
        proofs: PathBuf,
        /// Comma-separated TinyLlama tags, in the order the aggregator takes them, or `all`. Repeats
        /// only with `--mode compressed`.
        #[arg(long)]
        tags: String,
        #[arg(long, value_enum, required_unless_present = "execute_only")]
        mode: Option<Mode>,
        /// Where the bincode proof goes: a new file under the checkout's `runs/` or outside it.
        #[arg(long, required_unless_present = "execute_only")]
        out: Option<PathBuf>,
        /// With `--mode plonk` and `--tags all` only: writes `programVKey`, `publicValues` and
        /// `proofBytes` as 0x-hex JSON to this new file.
        #[arg(long)]
        fixture_out: Option<PathBuf>,
        /// Stops after the CPU checks: nothing touches a GPU.
        #[arg(long)]
        execute_only: bool,
    },
    /// Binding control B1 (`pv-byte`) or B2 (`vk`) (`SPEC.md` §2.1): proves the aggregator at `N = 1`
    /// on `B0L10` in compressed mode with one half of the `(vk, sha256(public values))` pair mutated.
    /// Exit 0 only if proving fails or the proof does not verify, 1 on a verifying proof.
    AggregateControl {
        #[arg(long, value_enum)]
        kind: ControlKind,
        /// The Docker-built segment guest (`pv-byte`).
        #[arg(long, required_if_eq("kind", "pv-byte"))]
        segment_elf: Option<PathBuf>,
        /// The Docker-built aggregator guest.
        #[arg(long)]
        aggregator_elf: PathBuf,
        /// The directory holding `B0L10.proof.bin` (`pv-byte`).
        #[arg(long, required_if_eq("kind", "pv-byte"))]
        proofs: Option<PathBuf>,
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
    /// Renders `TinyLlamaFixture.sol` from the 23 segments' `<tag>.public-values` (as
    /// `sp1-host execute --public-values` writes them), `tok_in` of `B0L0.fixed.json` and `tok_out` of
    /// `head.fixed.json`, after the assembly and grid checks; `modelId`, `eosTokenId` and the prompt
    /// and response text come from `tinyllama_fixture.rs`.
    FixtureFromBundles {
        /// The directory holding `<tag>.public-values` for `B0L0..B0L21` and `head`.
        #[arg(long)]
        public_values: PathBuf,
        /// The bundle directory holding `B0L0.fixed.json` and `head.fixed.json`.
        #[arg(long)]
        bundles: PathBuf,
        /// The prompt length `L`: `promptIds = tokIn[0..L]`, `answerIds = tokOut[L-1..S]`.
        #[arg(long)]
        prompt_len: usize,
        /// Where the rendered fixture goes; never under the checkout's `contracts/src`, and no symlink.
        #[arg(long)]
        out: PathBuf,
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
        Command::ExecuteAggregator { elf, public_values } => {
            let elf = match elf {
                Some(path) => read_elf(&path)?,
                None => AGGREGATOR_GUEST_ELF,
            };
            let model_id = tinyllama_model_id();
            let values = read_tinyllama_segment_values(&public_values)?;
            let native = aggregate(&model_id, &values)
                .map_err(|r| format!("aggregate rejects the public values: {r:?}"))?;
            let run = execute_aggregator(elf, aggregator_stdin(&model_id, &values))?;
            println!("exit code: {}", run.exit_code);
            println!("public values: {}", hex::encode(&run.public_values));
            println!("cycles: {}", run.cycles);
            println!("PGU: {}", run.gas);
            if run.exit_code != 0 {
                println!("stderr:\n{}", run.stderr);
                return Ok(ExitCode::FAILURE);
            }
            if run.public_values != native.public_digest {
                return Err(format!(
                    "the guest commits {}, not aggregate's publicDigest {}",
                    hex::encode(&run.public_values),
                    hex::encode(native.public_digest)
                ));
            }
            println!("OK: the guest commits aggregate's publicDigest");
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
            tags,
            out_dir,
            then,
        } => prove(&elf, &bundle, &tags, &out_dir, then.as_deref()),
        Command::Aggregate {
            segment_elf,
            aggregator_elf,
            proofs,
            tags,
            mode,
            out,
            fixture_out,
            execute_only,
        } => aggregate_command(AggregateArgs {
            segment_elf: &segment_elf,
            aggregator_elf: &aggregator_elf,
            proofs: &proofs,
            tags: &tags,
            mode,
            out: out.as_deref(),
            fixture_out: fixture_out.as_deref(),
            execute_only,
        }),
        Command::AggregateControl {
            kind,
            segment_elf,
            aggregator_elf,
            proofs,
        } => aggregate_control(
            kind,
            segment_elf.as_deref(),
            &aggregator_elf,
            proofs.as_deref(),
        ),
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
        Command::FixtureFromBundles {
            public_values,
            bundles,
            prompt_len,
            out,
        } => {
            let out = check_fixture_out(&out, &contracts_src())?;
            let rendered = fixture_from_bundles(&public_values, &bundles, prompt_len)?;
            fs::write(&out, rendered).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("fixture: {}", out.display());
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
