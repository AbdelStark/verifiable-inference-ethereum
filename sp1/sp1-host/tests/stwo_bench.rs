//! Runs the stwo-bench guest in the SP1 executor. Its cycle-tracker regions need sp1-sdk's
//! `profiling` executor: `cargo test --locked -p sp1-host --features profiling`.
#![cfg(feature = "profiling")]

use sp1_host::{CYCLE_LIMIT, STWO_BENCH_ELF};
use sp1_sdk::blocking::{ProverClient, prelude::*};

#[test]
fn runs_nitroo_stwo_without_prover_in_tracked_regions() {
    let n: u32 = 8;
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(n.to_le_bytes().to_vec());
    let client = ProverClient::builder().light().build();
    let (public_values, report) = client
        .execute(STWO_BENCH_ELF, stdin)
        .cycle_limit(CYCLE_LIMIT)
        .run()
        .expect("executor error");
    assert_eq!(report.exit_code, 0);
    // One 32-byte node hash, then two QM31 values of four M31 limbs each.
    assert_eq!(public_values.to_vec().len(), 32 + 2 * 4 * 4);
    for region in ["hash_node", "channel_draw", "qm31_mul_add"] {
        let cycles = report.cycle_tracker.get(region).copied();
        assert!(cycles.is_some_and(|c| c > 0), "region {region}: {cycles:?}");
        println!("stwo-bench N = {n}: {region} {} cycles", cycles.unwrap());
    }
}
