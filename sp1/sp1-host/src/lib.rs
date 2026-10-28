//! Host side of the SP1 guests: the segment guest's input encoding and executor run (`SPEC.md`
//! §1.1), and stwo-bench's.

use std::fs;
use std::io;
use std::path::Path;

use sp1_sdk::blocking::{ProverClient, prelude::*};
use sp1_sdk::{ExecutionReport, SP1Stdin};
use zk_prover::common::Proof;

/// The segment guest, built by `build.rs` with the local `succinct` toolchain. Only the Docker build
/// (`sp1/scripts/docker-guests.sh`) is reproducible, and only it is compared with the committed
/// `sp1/segment-guest.elf.sha256` and `sp1/segment-guest.vkey`.
pub const SEGMENT_GUEST_ELF: Elf = include_elf!("segment-guest");

/// The Stwo micro-benchmark guest: one verifier primitive of nitroo's Stwo without its `prover`
/// feature, run `N` times in its own cycle-tracker region (`STWO_BENCH_OPS`).
pub const STWO_BENCH_ELF: Elf = include_elf!("stwo-bench");

/// Bound on every guest run in the executor. `docs/EVM-GAS-MODEL.md` §6.3 estimates the largest
/// TinyLlama segment, `head`, at 3.319e9–5.730e9 cycles with its proof as `bincode`; this is twice
/// the top of that range, rounded up. SP1 v6.10.0 checks it once per gas chunk, so it is coarse.
pub const CYCLE_LIMIT: u64 = 12_000_000_000;

/// The segment guest's three input buffers (`SPEC.md` §1.1 "Guest input").
pub struct SegmentInput {
    /// `<tag>.fixed.json`, as is.
    pub fixed: Vec<u8>,
    /// `<tag>.claims.json`, as is.
    pub claims: Vec<u8>,
    /// The `StarkProof` of `<tag>.proof.json`, re-encoded as `bincode` 1.3.3.
    pub proof: Vec<u8>,
}

impl SegmentInput {
    /// Reads `<tag>.fixed.json`, `<tag>.claims.json` and `<tag>.proof.json` from `dir`.
    pub fn read_bundle(dir: &Path, tag: &str) -> io::Result<Self> {
        let read = |kind: &str| {
            let path = dir.join(format!("{tag}.{kind}.json"));
            fs::read(&path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))
        };
        Ok(Self {
            fixed: read("fixed")?,
            claims: read("claims")?,
            proof: proof_bincode(&read("proof")?).map_err(io::Error::other)?,
        })
    }

    /// The guest's stdin: the three buffers in order, each written with `write_vec`.
    pub fn stdin(&self) -> SP1Stdin {
        let mut stdin = SP1Stdin::new();
        for buf in [&self.fixed, &self.claims, &self.proof] {
            stdin.write_vec(buf.clone());
        }
        stdin
    }

    /// The size of the encoded input: the three buffers' bytes.
    pub fn len(&self) -> usize {
        self.fixed.len() + self.claims.len() + self.proof.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A `<tag>.proof.json` as the guest's proof buffer: the parsed `StarkProof`, `bincode::serialize`d.
/// Outside the trusted base: the guest verifies whatever proof it decodes.
pub fn proof_bincode(json: &[u8]) -> Result<Vec<u8>, String> {
    let proof: Proof = serde_json::from_slice(json).map_err(|e| format!("proof.json: {e}"))?;
    bincode::serialize(&proof).map_err(|e| e.to_string())
}

/// One run of a guest in the executor.
pub struct Execution {
    /// The guest's exit code: 0 on success, 1 on a panic. `execute` returns `Ok` either way.
    pub exit_code: u64,
    pub public_values: Vec<u8>,
    /// The last 2 KiB of the guest's stderr.
    pub stderr: String,
    /// `report.total_instruction_count()`.
    pub cycles: u64,
    /// `report.gas()`, in PGU.
    pub gas: u64,
    /// The process' peak RSS in bytes once the run returns (`peak_rss`).
    pub peak_rss: u64,
}

impl Execution {
    /// The `Reject` variant the segment guest's panic message names, if any.
    pub fn reject(&self) -> Option<&str> {
        self.stderr
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix("segment rejected: "))
    }
}

/// Runs `elf` on `stdin` in the executor (LightProver, gas on, [`CYCLE_LIMIT`]).
pub fn execute(elf: Elf, stdin: SP1Stdin) -> Result<Execution, String> {
    let client = ProverClient::builder().light().build();
    let (stderr_tx, stderr_rx) = tokio::sync::watch::channel(String::new());
    let (public_values, report): (_, ExecutionReport) = client
        .execute(elf, stdin)
        .calculate_gas(true)
        .cycle_limit(CYCLE_LIMIT)
        .stderr(stderr_tx)
        .run()
        .map_err(|e| e.to_string())?;
    let stderr = stderr_rx.borrow().clone();
    Ok(Execution {
        exit_code: report.exit_code,
        public_values: public_values.to_vec(),
        stderr,
        cycles: report.total_instruction_count(),
        gas: report.gas().ok_or("calculate_gas(true) reports no PGU")?,
        peak_rss: peak_rss(),
    })
}

/// The larger of `getrusage`'s `RUSAGE_SELF` and `RUSAGE_CHILDREN` `ru_maxrss`, in bytes. On Linux
/// x86_64 the native executor runs the guest in a child process, which `RUSAGE_SELF` misses.
pub fn peak_rss() -> u64 {
    let max_rss = |who| {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: `usage` is a valid out-pointer for one `rusage`, fully written on success.
        let rc = unsafe { libc::getrusage(who, usage.as_mut_ptr()) };
        assert_eq!(rc, 0, "getrusage: {}", io::Error::last_os_error());
        // SAFETY: getrusage succeeded.
        u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).unwrap_or(0)
    };
    // `ru_maxrss` is in bytes on macOS and in KiB elsewhere.
    let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
    max_rss(libc::RUSAGE_SELF).max(max_rss(libc::RUSAGE_CHILDREN)) * unit
}

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
