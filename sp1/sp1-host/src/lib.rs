//! Host side of the SP1 segment guest.

use sp1_sdk::{Elf, include_elf};

/// The segment guest, built by `build.rs` with the local `succinct` toolchain. Only the Docker build
/// (`make sp1-docker`) is reproducible.
pub const SEGMENT_GUEST_ELF: Elf = include_elf!("segment-guest");

/// The Stwo micro-benchmark guest: BLAKE2s Merkle hashing, channel draws and QM31 arithmetic of nitroo's
/// Stwo without its `prover` feature, each in a cycle-tracker region.
pub const STWO_BENCH_ELF: Elf = include_elf!("stwo-bench");

/// Bound on a guest run in the executor. SP1 v6.10.0 checks it once per gas chunk, so it is coarse.
pub const CYCLE_LIMIT: u64 = 100_000_000;
