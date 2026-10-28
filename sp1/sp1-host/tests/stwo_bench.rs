//! Runs every stwo-bench operation in the SP1 executor at a small `N`. Its cycle-tracker regions need
//! sp1-sdk's `profiling` executor: `cargo test --locked -p sp1-host --features profiling`.
#![cfg(feature = "profiling")]

use sp1_host::{CYCLE_LIMIT, STWO_BENCH_ELF, STWO_BENCH_OPS, stwo_bench_stdin};
use sp1_sdk::blocking::{ProverClient, prelude::*};

#[test]
fn runs_nitroo_stwo_without_prover_in_tracked_regions() {
    let n: u32 = 2;
    let client = ProverClient::builder().light().build();
    for (op, region) in STWO_BENCH_OPS.iter().enumerate() {
        // The decode operations get an empty buffer: every decode fails, and the region still runs.
        let (public_values, report) = client
            .execute(STWO_BENCH_ELF, stwo_bench_stdin(op, n, &[]))
            .cycle_limit(CYCLE_LIMIT)
            .run()
            .expect("executor error");
        assert_eq!(report.exit_code, 0, "{region}");
        let cycles = report.cycle_tracker.get(*region).copied();
        assert!(cycles.is_some_and(|c| c > 0), "region {region}: {cycles:?}");
        assert_eq!(report.invocation_tracker.get(*region), Some(&1), "{region}");
        let public_values = public_values.to_vec();
        if region.starts_with("decode_") {
            // Successful decodes, then the last proof's commitment and queried-value counts.
            assert_eq!(public_values, [0u8; 12], "{region}");
        } else {
            assert!(!public_values.is_empty(), "{region}");
        }
        println!("stwo-bench N = {n}: {region} {} cycles", cycles.unwrap());
    }
}
