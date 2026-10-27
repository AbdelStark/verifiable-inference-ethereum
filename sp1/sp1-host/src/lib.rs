//! Host side of the SP1 segment guest.

use sp1_sdk::{Elf, include_elf};

/// The segment guest, built by `build.rs` with the local `succinct` toolchain. Only the Docker build
/// (`make sp1-docker`) is reproducible.
pub const SEGMENT_GUEST_ELF: Elf = include_elf!("segment-guest");

/// Bound on a guest run in the executor. SP1 v6.10.0 checks it once per gas chunk, so it is coarse.
pub const CYCLE_LIMIT: u64 = 100_000_000;
