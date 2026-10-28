//! `sp1-host`: runs the segment guest on one bundle in the SP1 executor, or prints a guest's vkey.
//!
//! - `sp1-host execute [--elf PATH] --bundle DIR --tag T [--public-values PATH]`
//! - `sp1-host vkey --elf PATH`

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sp1_host::{SEGMENT_GUEST_ELF, SegmentInput, execute};
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
            let run = execute(elf, input.stdin())?;
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
    }
}

fn main() -> ExitCode {
    run(Cli::parse()).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    })
}
