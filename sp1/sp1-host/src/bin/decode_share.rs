//! `decode-share --bundle DIR --tag T`: decodes one segment's three guest buffers once in stwo-bench,
//! with `check_segment`'s types and calls (`SPEC.md` §1.1), and prints the cycles of its two
//! cycle-tracker regions: `Fixed` + `Claims` from JSON, and the proof from `bincode`. The regions
//! need sp1-sdk's profiling executor; the segment guest has none, which would change its ELF and
//! vkey. `make sp1-execute` runs it once per segment.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use sp1_host::{CYCLE_LIMIT, STWO_BENCH_ELF, SegmentInput, decode_segment_stdin};
use sp1_sdk::blocking::{ProverClient, prelude::*};

#[derive(Parser)]
#[command(about = "Cycles to decode one segment's guest buffers, in stwo-bench's regions")]
struct Cli {
    #[arg(long)]
    bundle: PathBuf,
    #[arg(long)]
    tag: String,
}

fn run(cli: Cli) -> Result<(), String> {
    let input = SegmentInput::read_bundle(&cli.bundle, &cli.tag).map_err(|e| e.to_string())?;
    let client = ProverClient::builder().light().build();
    let (public_values, report) = client
        .execute(STWO_BENCH_ELF, decode_segment_stdin(&input))
        .cycle_limit(CYCLE_LIMIT)
        .run()
        .map_err(|e| e.to_string())?;
    // Each buffer decoded, then the proof's commitment count.
    let pv = public_values.to_vec();
    if report.exit_code != 0 || pv.len() != 7 || pv[..3] != [1, 1, 1] || pv[3..] == [0; 4] {
        return Err(format!(
            "{}: stwo-bench did not decode all three buffers (exit {}, public values {})",
            cli.tag,
            report.exit_code,
            hex::encode(&pv)
        ));
    }
    let region = |name: &str| {
        report
            .cycle_tracker
            .get(name)
            .copied()
            .ok_or(format!("no {name} region"))
    };
    println!(
        "decode fixed+claims cycles: {}",
        region("decode_fixed_claims")?
    );
    println!("decode proof cycles: {}", region("decode_proof")?);
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
