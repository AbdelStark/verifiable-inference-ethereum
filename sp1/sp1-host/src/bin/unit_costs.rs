//! SP1 unit costs of SASH's verifier primitives (`docs/EVM-GAS-MODEL.md` §6). Runs each stwo-bench
//! operation alone in the profiling executor at two values of `N` and takes cycles and PGU as Δ/ΔN;
//! decodes the synthetic corpus' `sound_aligned` proof as JSON, bincode and postcard; prints the
//! tables as Markdown.
//!
//! `cargo run --release --locked -p sp1-host --features profiling --bin unit-costs -- [CORPUS_DIR]`
//!
//! `CORPUS_DIR` (default `sp1/testdata/synthetic`) holds the synthetic corpus of `make sash-synthetic`,
//! checked against `sp1/testdata/synthetic.sha256`. From CI: `gh run download <run-id> -n
//! sash-synthetic-corpus -D <dir>`, then pass `<dir>/synthetic`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sp1_host::{STWO_BENCH_ELF, STWO_BENCH_OPS, stwo_bench_stdin};
use sp1_sdk::blocking::{ProverClient, prelude::*};
use sp1_sdk::{ExecutionReport, SP1_CIRCUIT_VERSION};
use stwo::core::proof::StarkProof;
use stwo::core::vcs::blake2_merkle::Blake2sM31MerkleHasher;

/// SASH's proof type (`Proof` in `prover/src/common.rs`).
type Proof = StarkProof<Blake2sM31MerkleHasher>;

const PINS: &str = include_str!("../../../testdata/synthetic.sha256");
const MEASURED: &str = "sound_aligned";
/// The sp1-sdk version `sp1/Cargo.toml` pins.
const SP1_SDK: &str = "6.10.0";
/// Bound on one measurement run.
const CYCLE_LIMIT: u64 = 50_000_000_000;
/// The `hash_node_leaf*` operations: M31s per leaf, region.
const LEAVES: [(u64, &str); 5] = [
    (4, "hash_node_leaf4"),
    (16, "hash_node_leaf16"),
    (20, "hash_node_leaf20"),
    (256, "hash_node_leaf256"),
    (4096, "hash_node_leaf4096"),
];

/// The `N` pair each operation runs at.
fn n_pair(op: &str) -> (u32, u32) {
    match op {
        "hash_node_leaf256" => (500, 1_000),
        "hash_node_leaf4096" => (32, 64),
        "qm31_add" => (1_000_000, 2_000_000),
        "qm31_mul" | "qm31_mul_add" | "m31_qm31_mul" => (200_000, 400_000),
        "row_quotients" => (100, 200),
        op if op.starts_with("decode_") => (1, 2),
        _ => (5_000, 10_000),
    }
}

/// One operation's two runs.
struct Unit {
    n: (u32, u32),
    cycles: (u64, u64),
    pgu: (u64, u64),
    region: (u64, u64),
    public_values: Vec<u8>,
}

impl Unit {
    fn dn(&self) -> f64 {
        f64::from(self.n.1 - self.n.0)
    }
    fn cycles(&self) -> f64 {
        (self.cycles.1 - self.cycles.0) as f64 / self.dn()
    }
    fn pgu(&self) -> f64 {
        (self.pgu.1 - self.pgu.0) as f64 / self.dn()
    }
    fn region(&self) -> f64 {
        (self.region.1 - self.region.0) as f64 / self.dn()
    }
}

/// A cost in cycles and PGU.
#[derive(Clone, Copy)]
struct Cost {
    cycles: f64,
    pgu: f64,
}

impl Cost {
    fn of(unit: &Unit) -> Self {
        Self {
            cycles: unit.cycles(),
            pgu: unit.pgu(),
        }
    }
}

fn main() {
    let dir = std::env::args().nth(1).map_or_else(
        || {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../testdata/synthetic"
            ))
        },
        PathBuf::from,
    );
    check_corpus(&dir);

    // Decode inputs and the round-trip check, natively.
    let json = read(&dir, &format!("{MEASURED}.proof.json"));
    let parsed: Proof = serde_json::from_slice(&json).expect("proof.json parses");
    let reference = serde_json::to_vec(&parsed).unwrap();
    let encodings: [(&str, Vec<u8>); 3] = [
        ("json", json.clone()),
        ("bincode", bincode::serialize(&parsed).unwrap()),
        ("postcard", postcard::to_allocvec(&parsed).unwrap()),
    ];
    for (name, bytes) in &encodings {
        let decoded: Proof = match *name {
            "json" => serde_json::from_slice(bytes).unwrap(),
            "bincode" => bincode::deserialize(bytes).unwrap(),
            _ => postcard::from_bytes(bytes).unwrap(),
        };
        assert!(
            serde_json::to_vec(&decoded).unwrap() == reference,
            "{name} does not round-trip"
        );
    }
    let queried: u64 = parsed.0.queried_values.iter().map(|t| t.len() as u64).sum();

    // Executor runs.
    let client = ProverClient::builder().light().build();
    let mut units = HashMap::new();
    for (op, name) in STWO_BENCH_OPS.iter().enumerate() {
        let buf: &[u8] = name.strip_prefix("decode_").map_or(&[], |enc| {
            &encodings.iter().find(|(e, _)| *e == enc).unwrap().1
        });
        let n = n_pair(name);
        let (pv0, r0) = execute(&client, op, n.0, buf);
        let (pv1, r1) = execute(&client, op, n.1, buf);
        if !buf.is_empty() {
            let expect = [n.1, parsed.0.commitments.len() as u32, queried as u32]
                .map(u32::to_le_bytes)
                .concat();
            assert_eq!(
                pv1, expect,
                "{name}: the guest's decode differs from the host's"
            );
            assert_eq!(&pv0[4..], &expect[4..], "{name}");
        }
        let unit = Unit {
            n,
            cycles: (r0.total_instruction_count(), r1.total_instruction_count()),
            pgu: (r0.gas().unwrap(), r1.gas().unwrap()),
            region: (r0.cycle_tracker[*name], r1.cycle_tracker[*name]),
            public_values: pv1,
        };
        eprintln!(
            "{name}: {:.1} cycles, {:.1} PGU per op",
            unit.cycles(),
            unit.pgu()
        );
        units.insert(*name, unit);
    }

    println!(
        "sp1-sdk {SP1_SDK} (circuit version {SP1_CIRCUIT_VERSION}), portable executor (sp1-sdk `profiling`), {}\n",
        machine()
    );
    print_units(&units);
    print_fit(&units);
    print_decode(&units, &encodings, &reference);
}

fn execute(
    client: &sp1_sdk::blocking::LightProver,
    op: usize,
    n: u32,
    buf: &[u8],
) -> (Vec<u8>, ExecutionReport) {
    let (public_values, report) = client
        .execute(STWO_BENCH_ELF, stwo_bench_stdin(op, n, buf))
        .calculate_gas(true)
        .cycle_limit(CYCLE_LIMIT)
        .run()
        .expect("executor error");
    assert_eq!(report.exit_code, 0, "{}", STWO_BENCH_OPS[op]);
    (public_values.to_vec(), report)
}

/// Checks every corpus file against `sp1/testdata/synthetic.sha256`.
fn check_corpus(dir: &Path) {
    for line in PINS.lines() {
        let (hash, file) = line.split_once("  ").expect("sha256sum line");
        assert_eq!(
            hex::encode(Sha256::digest(read(dir, file))),
            hash,
            "{file} differs from its pin"
        );
    }
}

fn read(dir: &Path, file: &str) -> Vec<u8> {
    let path = dir.join(file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn machine() -> String {
    let sysctl = |key| {
        std::process::Command::new("sysctl")
            .args(["-n", key])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let cpu = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| std::env::consts::ARCH.into());
    let mem = sysctl("hw.memsize").and_then(|m| m.parse::<u64>().ok());
    match mem {
        Some(m) => format!("{cpu}, {} GB, {}", m >> 30, std::env::consts::OS),
        None => format!("{cpu}, {}", std::env::consts::OS),
    }
}

fn print_units(units: &HashMap<&str, Unit>) {
    println!(
        "| Operation | N₁ / N₂ | Cycles at N₁ / N₂ | PGU at N₁ / N₂ | Cycles per op (Δ/ΔN) | PGU per op (Δ/ΔN) | Region cycles per op (cross-check) |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for name in STWO_BENCH_OPS.iter().filter(|n| !n.starts_with("decode_")) {
        let u = &units[name];
        println!(
            "| `{name}` | {} / {} | {} / {} | {} / {} | {} | {} | {} |",
            int(u.n.0.into()),
            int(u.n.1.into()),
            int(u.cycles.0),
            int(u.cycles.1),
            int(u.pgu.0),
            int(u.pgu.1),
            dec(u.cycles()),
            dec(u.pgu()),
            dec(u.region()),
        );
    }
    println!();
}

/// 64-byte BLAKE2s blocks of a `hash_node` over `m` M31s and no children.
fn blocks(m: u64) -> u64 {
    (4 * m).div_ceil(64)
}

/// Least squares `y = c0 + k × blocks + u × m` over `(blocks, m, y)`: `[c0, k, u]`.
fn fit_node(points: &[(f64, f64, f64)]) -> [f64; 3] {
    // Normal equations, augmented, solved by Gauss-Jordan elimination.
    let mut a = [[0.0f64; 4]; 3];
    for &(b, m, y) in points {
        let x = [1.0, b, m, y];
        for (row, xi) in a.iter_mut().zip(x) {
            for (cell, xj) in row.iter_mut().zip(x) {
                *cell += xi * xj;
            }
        }
    }
    for c in 0..3 {
        let pivot = a[c];
        for (r, row) in a.iter_mut().enumerate() {
            if r != c {
                let f = row[c] / pivot[c];
                for (cell, p) in row.iter_mut().zip(pivot) {
                    *cell -= f * p;
                }
            }
        }
    }
    [a[0][3] / a[0][0], a[1][3] / a[1][1], a[2][3] / a[2][2]]
}

fn print_fit(units: &HashMap<&str, Unit>) {
    let points = |get: fn(&Unit) -> f64| -> Vec<(f64, f64, f64)> {
        LEAVES
            .iter()
            .map(|(m, name)| (blocks(*m) as f64, *m as f64, get(&units[name])))
            .collect()
    };
    let [node_c, block_c, update_c] = fit_node(&points(Unit::cycles));
    let [node_p, block_p, update_p] = fit_node(&points(Unit::pgu));
    let steps = u32::from_le_bytes(
        units["row_quotients"].public_values[..4]
            .try_into()
            .unwrap(),
    );
    let compress = Cost::of(&units["blake2s_compress"]);
    println!(
        "**`hash_node` leaf fit** (`per node + per block × ⌈4m / 64⌉ + per M31 × m`, least squares over m = 4, 16, 20, 256 and 4,096 M31s):\n"
    );
    println!("| Term | Cycles | PGU |\n|---|---:|---:|");
    println!("| Per node | {} | {} |", dec(node_c), dec(node_p));
    println!(
        "| Per 64-byte block | {} | {} |",
        dec(block_c),
        dec(block_p)
    );
    println!(
        "| Bare compression (`blake2s_compress`, cross-check of the block term) | {} | {} |",
        dec(compress.cycles),
        dec(compress.pgu)
    );
    println!(
        "| Per M31 (`update`) | {} | {} |",
        dec(update_c),
        dec(update_p)
    );
    for (m, name) in LEAVES {
        let b = blocks(m) as f64;
        let fitted = Cost {
            cycles: node_c + block_c * b + update_c * m as f64,
            pgu: node_p + block_p * b + update_p * m as f64,
        };
        let measured = Cost::of(&units[name]);
        println!(
            "| Fit residual at {} M31s ({} block{}) | {:+.2}% | {:+.2}% |",
            int(m),
            blocks(m),
            if blocks(m) == 1 { "" } else { "s" },
            100.0 * (measured.cycles - fitted.cycles) / fitted.cycles,
            100.0 * (measured.pgu - fitted.pgu) / fitted.pgu
        );
    }
    println!(
        "| `accumulate_row_quotients` per (column, sample) step ({} steps per row) | {} | {} |\n",
        int(steps.into()),
        dec(units["row_quotients"].cycles() / f64::from(steps)),
        dec(units["row_quotients"].pgu() / f64::from(steps)),
    );
}

fn print_decode(units: &HashMap<&str, Unit>, encodings: &[(&str, Vec<u8>); 3], reference: &[u8]) {
    println!(
        "**Decoding `{MEASURED}.proof.json`'s `StarkProof`** (N₁ / N₂ = 1 / 2 decodes; round trip: `serde_json::to_vec` of the decoded proof equals that of the proof parsed from `proof.json`, {} B):\n",
        int(reference.len() as u64)
    );
    println!(
        "| Encoding | Size | Size / JSON | Cycles at N₁ / N₂ | Cycles per decode | Cycles per input byte | PGU per input byte | Region cycles per decode | Round trip |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---|");
    for (enc, bytes) in encodings {
        let u = &units[format!("decode_{enc}").as_str()];
        println!(
            "| {enc} | {} B | {:.3} | {} / {} | {} | {} | {} | {} | yes |",
            int(bytes.len() as u64),
            bytes.len() as f64 / encodings[0].1.len() as f64,
            int(u.cycles.0),
            int(u.cycles.1),
            dec(u.cycles()),
            dec(u.cycles() / bytes.len() as f64),
            dec(u.pgu() / bytes.len() as f64),
            dec(u.region()),
        );
    }
    println!();
}

fn int(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn dec(v: f64) -> String {
    if v >= 100.0 {
        int(v.round() as u64)
    } else {
        format!("{v:.2}")
    }
}
