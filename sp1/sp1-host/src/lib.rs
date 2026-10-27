//! Host side of the SP1 segment guest.

use sp1_sdk::{Elf, SP1Stdin, include_elf};

/// The segment guest, built by `build.rs` with the local `succinct` toolchain. Only the Docker build
/// (`make sp1-docker`) is reproducible.
pub const SEGMENT_GUEST_ELF: Elf = include_elf!("segment-guest");

/// The Stwo micro-benchmark guest: one verifier primitive of nitroo's Stwo without its `prover`
/// feature, run `N` times in its own cycle-tracker region (`STWO_BENCH_OPS`).
pub const STWO_BENCH_ELF: Elf = include_elf!("stwo-bench");

/// stwo-bench's operations: the selector is the index, the name its cycle-tracker region. Keep in
/// step with the `match` in `stwo-bench/src/main.rs`.
pub const STWO_BENCH_OPS: [&str; 16] = [
    "blake2s_compress",
    "hash_node_leaf4",
    "hash_node_leaf16",
    "hash_node_leaf20",
    "hash_node_leaf256",
    "hash_node_leaf4096",
    "hash_node_children",
    "channel_draw",
    "qm31_add",
    "qm31_mul",
    "qm31_mul_add",
    "m31_qm31_mul",
    "row_quotients",
    "decode_json",
    "decode_bincode",
    "decode_postcard",
];

/// stwo-bench's stdin: `op` and `n`, then `proof` for the `decode_*` operations only.
pub fn stwo_bench_stdin(op: usize, n: u32, proof: &[u8]) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    let header = [(op as u32).to_le_bytes(), n.to_le_bytes()].concat();
    stdin.write_vec(header);
    if STWO_BENCH_OPS[op].starts_with("decode_") {
        stdin.write_vec(proof.to_vec());
    }
    stdin
}

/// Bound on a guest run in the executor. SP1 v6.10.0 checks it once per gas chunk, so it is coarse.
pub const CYCLE_LIMIT: u64 = 100_000_000;
