//! `native-check --bundle DIR --tag T --public-values PATH`: runs the segment check natively on
//! `<T>.{fixed,claims,proof}.json`, with the proof re-encoded as the guest gets it, and fails unless
//! its public values equal the 226 bytes the executor wrote to `PATH`. Prints `fixedHash` and the
//! BLAKE2s compressions the check ran, counted by nitroo's Stwo under `count-ops` (`sp1/patches/0005`):
//! `hash_node` and the channel apart. `make sp1-execute` runs it once per segment.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use sash_segment_check::check_segment;
use sp1_host::{BLAKE2S_COMPRESSION_CYCLES, SegmentInput};
use stwo::core::vcs::count_ops::{counts, reset_counts};

#[derive(Parser)]
#[command(about = "Native segment check with BLAKE2s counts, against the executor's public values")]
struct Cli {
    #[arg(long)]
    bundle: PathBuf,
    #[arg(long)]
    tag: String,
    /// The executor's 226 committed bytes for this segment.
    #[arg(long)]
    public_values: PathBuf,
}

fn run(cli: Cli) -> Result<(), String> {
    let input = SegmentInput::read_bundle(&cli.bundle, &cli.tag).map_err(|e| e.to_string())?;
    let executed = fs::read(&cli.public_values)
        .map_err(|e| format!("{}: {e}", cli.public_values.display()))?;
    reset_counts();
    let values = check_segment(&input.fixed, &input.claims, &input.proof)
        .map_err(|reject| format!("{}: segment rejected natively: {reject}", cli.tag))?;
    let (hash_node, channel) = counts();
    let native = values.encode();
    if native[..] != executed[..] {
        return Err(format!(
            "{}: the executor's public values differ from native check_segment's\n  executor: {}\n  native:   {}",
            cli.tag,
            hex::encode(&executed),
            hex::encode(native)
        ));
    }
    println!("public values: equal to the executor's");
    println!("fixedHash: {}", hex::encode(values.fixed_hash));
    println!("hash_node compressions: {hash_node}");
    println!("channel compressions: {channel}");
    println!(
        "BLAKE2s cycles: {} (compressions x {BLAKE2S_COMPRESSION_CYCLES})",
        (hash_node + channel) * BLAKE2S_COMPRESSION_CYCLES
    );
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
