//! SP1 unit costs of SASH's verifier primitives, and their extrapolation to TinyLlama
//! (`docs/EVM-GAS-MODEL.md` §6). Runs each stwo-bench operation alone in the profiling executor at two
//! values of `N` and takes cycles and PGU as Δ/ΔN; decodes the synthetic corpus' `sound_aligned`
//! proof as JSON, bincode and postcard; prints the tables as Markdown.
//!
//! `cargo run --release --locked -p sp1-host --features profiling --bin unit-costs -- [CORPUS_DIR]`
//!
//! `CORPUS_DIR` (default `sp1/testdata/synthetic`) holds the synthetic corpus of `make sash-synthetic`,
//! checked against `sp1/testdata/synthetic.sha256`.

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
/// The bundles cross-checked against the full `verify_composed` cycles of #41's scratch guest.
const CROSS_CHECKED: [&str; 2] = ["sound_aligned", "aligned_gkr5"];
/// The sp1-sdk version `sp1/Cargo.toml` pins.
const SP1_SDK: &str = "6.10.0";
/// Bound on one measurement run.
const CYCLE_LIMIT: u64 = 50_000_000_000;
/// SASH's query count (`n_queries`, SPEC.md §1.1 rule 2).
const QUERIES: u64 = 100;
/// The prover network's per-request cap, in PGU.
const PGU_CAP: f64 = 1e12;
/// Unverified: Succinct's Hypercube figure, ~400M PGU in under 12 s on 16 RTX 5090s.
const PGU_PER_GPU_SECOND: f64 = 2e6;

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

/// `docs/EVM-GAS-MODEL.md` §1, per column: `B0L0`, `B0L10`, `head`, all 23 segments.
struct Segment {
    name: &'static str,
    segments: u64,
    trees: u64,
    columns: u64,
    fri_inner_layers: u64,
    json_bytes: u64,
    sampled: u64,
    leaf_m31s: u64,
    siblings: u64,
}

const TINYLLAMA: [Segment; 4] = [
    Segment {
        name: "`B0L0`",
        segments: 1,
        trees: 10,
        columns: 20_911,
        fri_inner_layers: 25,
        json_bytes: 33_816_971,
        sampled: 21_718,
        leaf_m31s: 2_940_131,
        siblings: 31_151,
    },
    Segment {
        name: "`B0L10`",
        segments: 1,
        trees: 10,
        columns: 19_612,
        fri_inner_layers: 20,
        json_bytes: 29_389_111,
        sampled: 20_406,
        leaf_m31s: 2_630_253,
        siblings: 20_305,
    },
    Segment {
        name: "`head`",
        segments: 1,
        trees: 6,
        columns: 68_484,
        fri_inner_layers: 20,
        json_bytes: 84_613_678,
        sampled: 69_560,
        leaf_m31s: 8_099_813,
        siblings: 19_427,
    },
    Segment {
        name: "All 23",
        segments: 23,
        trees: 226,
        columns: 501_197,
        fri_inner_layers: 485,
        json_bytes: 761_015_496,
        sampled: 519_784,
        leaf_m31s: 65_967_346,
        siblings: 518_489,
    },
];

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
    fn per(self, d: f64) -> Self {
        self.times(1.0 / d)
    }
    fn times(self, k: f64) -> Self {
        Self {
            cycles: self.cycles * k,
            pgu: self.pgu * k,
        }
    }
    fn plus(self, o: Self) -> Self {
        Self {
            cycles: self.cycles + o.cycles,
            pgu: self.pgu + o.pgu,
        }
    }
}

/// Counts in one proof, the inputs of the analytic model.
struct ProofCounts {
    trees: usize,
    columns: u64,
    sampled: u64,
    two_sample_columns: u64,
    positions: u64,
    leaf_m31s: u64,
    fri_layers: u64,
    siblings: u64,
}

impl ProofCounts {
    fn of(proof: &Proof) -> Self {
        let p = &proof.0;
        let columns = p.sampled_values.iter().map(|t| t.len() as u64).sum();
        let sampled = p
            .sampled_values
            .iter()
            .flatten()
            .map(|c| c.len() as u64)
            .sum();
        assert!(p.sampled_values.iter().flatten().all(|c| c.len() <= 2));
        let two_sample_columns = p
            .sampled_values
            .iter()
            .flatten()
            .filter(|c| c.len() == 2)
            .count();
        // The composition tree's columns have the lifting size: one value per query position each.
        let (last_values, last_columns) = (
            p.queried_values.last().unwrap().len(),
            p.sampled_values.last().unwrap().len(),
        );
        assert!(last_values.is_multiple_of(last_columns));
        let fri = &p.fri_proof;
        let fri_layers = std::iter::once(&fri.first_layer).chain(&fri.inner_layers);
        let fri_witness: u64 = fri_layers.clone().map(|l| l.fri_witness.len() as u64).sum();
        let fri_layer_count = 1 + fri.inner_layers.len() as u64;
        let tree_m31s: u64 = p.queried_values.iter().map(|t| t.len() as u64).sum::<u64>()
            + p.decommitments
                .iter()
                .map(|d| d.column_witness.len() as u64)
                .sum::<u64>();
        Self {
            trees: p.commitments.len(),
            columns,
            sampled,
            two_sample_columns: two_sample_columns as u64,
            positions: (last_values / last_columns) as u64,
            // FRI leaves: one QM31 per witness value and (bound) per query position and layer.
            leaf_m31s: tree_m31s + 4 * (fri_witness + QUERIES * fri_layer_count),
            fri_layers: fri_layer_count,
            siblings: p
                .decommitments
                .iter()
                .map(|d| d.hash_witness.len() as u64)
                .sum::<u64>()
                + fri_layers
                    .map(|l| l.decommitment.hash_witness.len() as u64)
                    .sum::<u64>(),
        }
    }
}

/// The measured per-unit costs the model multiplies.
struct Model {
    per_m31: Cost,
    per_node: Cost,
    children: Cost,
    step: Cost,
    qm31_mul: Cost,
    qm31_add: Cost,
    m31_qm31_mul: Cost,
    /// Per byte, by encoding.
    decode: Vec<(&'static str, Cost, f64)>,
}

impl Model {
    /// BLAKE2s: leaf values, bottom nodes (bound: `QUERIES` per tree, two per query per FRI layer) and
    /// internal nodes, taken as one two-children `hash_node` per sibling hash.
    fn blake(&self, leaf_m31s: u64, bottom_nodes: u64, siblings: u64) -> (Cost, Cost) {
        let leaf = self
            .per_m31
            .times(leaf_m31s as f64)
            .plus(self.per_node.times(bottom_nodes as f64));
        (leaf, self.children.times(siblings as f64))
    }
    /// One FRI fold's field arithmetic: 1 QM31 mul, 1 M31 × QM31 mul, 2 QM31 adds.
    fn fold(&self) -> Cost {
        self.qm31_mul
            .plus(self.m31_qm31_mul)
            .plus(self.qm31_add.times(2.0))
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
    let sizes = check_corpus(&dir);

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
    let counts = ProofCounts::of(&parsed);
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
            let expect = [n.1, counts.trees as u32, queried as u32]
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
    let model = fit(&units, &encodings);
    print_decode(&units, &encodings, &reference);
    print_counts(&counts);
    let share = proof_share(&sizes);
    print_extrapolation(&model, share);
    print_cross_check(&model, &dir, &sizes);
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

/// Checks every corpus file against `sp1/testdata/synthetic.sha256`; returns their sizes.
fn check_corpus(dir: &Path) -> HashMap<String, u64> {
    PINS.lines()
        .map(|line| {
            let (hash, file) = line.split_once("  ").expect("sha256sum line");
            let bytes = read(dir, file);
            assert_eq!(
                hex::encode(Sha256::digest(&bytes)),
                hash,
                "{file} differs from its pin"
            );
            (file.to_string(), bytes.len() as u64)
        })
        .collect()
}

fn read(dir: &Path, file: &str) -> Vec<u8> {
    let path = dir.join(file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The `proof.json` share of the corpus' bundle bytes (`fixed`, `claims`, `proof`).
fn proof_share(sizes: &HashMap<String, u64>) -> f64 {
    let proof: u64 = sizes
        .iter()
        .filter(|(f, _)| f.ends_with(".proof.json"))
        .map(|(_, s)| s)
        .sum();
    proof as f64 / sizes.values().sum::<u64>() as f64
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

/// Least-squares line through `(x, y)`: `(intercept, slope)`.
fn line(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    let (mx, my) = (sx / n, sy / n);
    let sxy: f64 = points.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    let sxx: f64 = points.iter().map(|(x, _)| (x - mx) * (x - mx)).sum();
    let slope = sxy / sxx;
    (my - slope * mx, slope)
}

fn fit(units: &HashMap<&str, Unit>, encodings: &[(&'static str, Vec<u8>); 3]) -> Model {
    let leaves = [
        (16.0, "hash_node_leaf16"),
        (256.0, "hash_node_leaf256"),
        (4096.0, "hash_node_leaf4096"),
    ];
    let cycles: Vec<_> = leaves
        .iter()
        .map(|(m, n)| (*m, units[n].cycles()))
        .collect();
    let pgu: Vec<_> = leaves.iter().map(|(m, n)| (*m, units[n].pgu())).collect();
    let (node_c, m31_c) = line(&cycles);
    let (node_p, m31_p) = line(&pgu);
    let steps = u32::from_le_bytes(
        units["row_quotients"].public_values[..4]
            .try_into()
            .unwrap(),
    );
    let compress = units["blake2s_compress"].cycles();
    println!(
        "**`hash_node` leaf fit** (`cycles = per node + per M31 × M31s`, least squares over 16, 256 and 4,096 M31s):\n"
    );
    println!("| Term | Cycles | PGU |\n|---|---:|---:|");
    println!("| Per M31 | {} | {} |", dec(m31_c), dec(m31_p));
    println!("| Per node | {} | {} |", dec(node_c), dec(node_p));
    for (m, name) in leaves {
        let fitted = node_c + m31_c * m;
        println!(
            "| Fit residual at {m} M31s | {:+.2}% | |",
            100.0 * (units[name].cycles() - fitted) / fitted
        );
    }
    println!(
        "| Bare compression ×1/16 per M31 (`blake2s_compress` {} / 16) | {} | |",
        dec(compress),
        dec(compress / 16.0)
    );
    println!(
        "| `accumulate_row_quotients` per (column, sample) step ({} steps per row) | {} | {} |\n",
        steps,
        dec(units["row_quotients"].cycles() / f64::from(steps)),
        dec(units["row_quotients"].pgu() / f64::from(steps)),
    );
    Model {
        per_m31: Cost {
            cycles: m31_c,
            pgu: m31_p,
        },
        per_node: Cost {
            cycles: node_c,
            pgu: node_p,
        },
        children: Cost::of(&units["hash_node_children"]),
        step: Cost::of(&units["row_quotients"]).per(f64::from(steps)),
        qm31_mul: Cost::of(&units["qm31_mul"]),
        qm31_add: Cost::of(&units["qm31_add"]),
        m31_qm31_mul: Cost::of(&units["m31_qm31_mul"]),
        decode: encodings
            .iter()
            .map(|(enc, bytes)| {
                let u = &units[format!("decode_{enc}").as_str()];
                let ratio = bytes.len() as f64 / encodings[0].1.len() as f64;
                (*enc, Cost::of(u).per(bytes.len() as f64), ratio)
            })
            .collect(),
    }
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

fn print_counts(c: &ProofCounts) {
    let exact = c.positions * (c.sampled + c.two_sample_columns);
    println!(
        "**Quotient-step count on `{MEASURED}`:** {} trees, {} columns, {} sampled values, {} columns with two samples, {} query positions. Exact steps: positions × (sampled values + two-sample columns, each of which adds a periodicity sample in `build_samples_with_randomness_and_periodicity`) = {}; `≤ 100 × sampled values` gives {} ({:+.2}%).\n",
        c.trees,
        int(c.columns),
        int(c.sampled),
        int(c.two_sample_columns),
        c.positions,
        int(exact),
        int(QUERIES * c.sampled),
        100.0 * (QUERIES * c.sampled) as f64 / exact as f64 - 100.0
    );
}

/// Steps per segment: `QUERIES` × (sampled values + two-sample columns), taking every column with more
/// than one sample to have exactly two, as on the synthetic proof.
fn steps(sampled: u64, columns: u64) -> u64 {
    QUERIES * (sampled + (sampled - columns))
}

fn print_extrapolation(model: &Model, share: f64) {
    println!(
        "**Extrapolation to TinyLlama** (`docs/EVM-GAS-MODEL.md` §1 counts; proof.json share of the JSON bytes taken from the synthetic corpus: {:.2}%):\n",
        100.0 * share
    );
    let rows: Vec<(String, Vec<Cost>)> = {
        let mut rows: Vec<(String, Vec<Cost>)> = Vec::new();
        let mut leaf = Vec::new();
        let mut internal = Vec::new();
        let mut quot = Vec::new();
        let mut rest = Vec::new();
        let mut proof: Vec<Vec<Cost>> = vec![Vec::new(); model.decode.len()];
        let mut fold = Vec::new();
        for s in &TINYLLAMA {
            let bottom = QUERIES * s.trees + 2 * QUERIES * (s.fri_inner_layers + s.segments);
            let (l, i) = model.blake(s.leaf_m31s, bottom, s.siblings);
            leaf.push(l);
            internal.push(i);
            quot.push(model.step.times(steps(s.sampled, s.columns) as f64));
            let json = s.json_bytes as f64;
            rest.push(model.decode[0].1.times(json * (1.0 - share)));
            for (k, (_, per_byte, ratio)) in model.decode.iter().enumerate() {
                proof[k].push(per_byte.times(json * share * ratio));
            }
            fold.push(
                model
                    .fold()
                    .times((QUERIES * (s.fri_inner_layers + s.segments)) as f64),
            );
        }
        rows.push(("BLAKE2s, leaves: per M31 × leaf M31s + per node × bottom nodes (≤ 100 per tree, 200 per FRI layer)".into(), leaf));
        rows.push((
            "BLAKE2s, internal nodes: `hash_node` (two children) × sibling hashes".into(),
            internal,
        ));
        rows.push((
            "DEEP quotients: per step × 100 × (sampled values + two-sample columns)".into(),
            quot,
        ));
        for (k, (enc, _, _)) in model.decode.iter().enumerate() {
            rows.push((format!("Decode `proof` as {enc}"), proof[k].clone()));
        }
        rows.push(("Decode `fixed` + `claims` (JSON)".into(), rest));
        rows.push(("FRI folds, field arithmetic only (bound)".into(), fold));
        rows
    };
    let total = |enc: usize| -> Vec<Cost> {
        (0..TINYLLAMA.len())
            .map(|i| {
                [0, 1, 2, 3 + enc, 3 + model.decode.len()].iter().fold(
                    Cost {
                        cycles: 0.0,
                        pgu: 0.0,
                    },
                    |a, &r| a.plus(rows[r].1[i]),
                )
            })
            .collect()
    };
    let header = TINYLLAMA
        .iter()
        .map(|s| s.name)
        .collect::<Vec<_>>()
        .join(" | ");
    let align = TINYLLAMA
        .iter()
        .map(|_| "---:")
        .collect::<Vec<_>>()
        .join("|");
    for (metric, get) in [
        ("Cycles", (|c: &Cost| c.cycles) as fn(&Cost) -> f64),
        ("PGU", |c: &Cost| c.pgu),
    ] {
        println!("| {metric} | {header} |\n|---|{align}|");
        for (label, costs) in &rows {
            println!(
                "| {label} | {} |",
                costs
                    .iter()
                    .map(|c| sci(get(c)))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
        for (k, (enc, _, _)) in model.decode.iter().enumerate() {
            let t = total(k);
            println!(
                "| **Measured parts, `proof` as {enc}** | {} |",
                t.iter()
                    .map(|c| format!("**{}**", sci(get(c))))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
        println!(
            "| Unmeasured: OODS constraint evaluation, FRI fold domain points and inversions, the channel, GKR | — | — | — | — |\n"
        );
    }
    println!(
        "| Against the per-request cap (one request per segment) and GPU time | {header} |\n|---|{align}|"
    );
    for (k, (enc, _, _)) in model.decode.iter().enumerate() {
        let t = total(k);
        println!(
            "| `proof` as {enc}: PGU / 1e12 cap | {} |",
            t.iter()
                .map(|c| format!("{:.2}%", 100.0 * c.pgu / PGU_CAP))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        println!(
            "| `proof` as {enc}: GPU-hours at 2M PGU/s per GPU (unverified) | {} |",
            t.iter()
                .map(|c| format!("{:.3}", c.pgu / PGU_PER_GPU_SECOND / 3600.0))
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    println!();
}

/// The same model on the synthetic bundles #41 ran `verify_composed` on, with the JSON decode of all
/// three buffers, as that guest parsed them.
fn print_cross_check(model: &Model, dir: &Path, sizes: &HashMap<String, u64>) {
    println!(
        "**Cross-check on synthetic bundles** (counts from each `proof.json`; decode = JSON cycles per byte × the three files' bytes):\n"
    );
    println!(
        "| Bundle | Leaf M31s | Bottom nodes (bound) | Sibling hashes | Quotient steps | JSON bytes | BLAKE2s cycles | Quotient cycles | JSON decode cycles | Measured parts |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for tag in CROSS_CHECKED {
        let proof: Proof =
            serde_json::from_slice(&read(dir, &format!("{tag}.proof.json"))).unwrap();
        let c = ProofCounts::of(&proof);
        let bottom = QUERIES * c.trees as u64 + 2 * QUERIES * c.fri_layers;
        let (leaf, internal) = model.blake(c.leaf_m31s, bottom, c.siblings);
        let steps = c.positions * (c.sampled + c.two_sample_columns);
        let quot = model.step.times(steps as f64);
        let json: u64 = ["fixed", "claims", "proof"]
            .iter()
            .map(|k| sizes[&format!("{tag}.{k}.json")])
            .sum();
        let decode = model.decode[0].1.times(json as f64);
        let blake = leaf.plus(internal);
        println!(
            "| `{tag}` | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            int(c.leaf_m31s),
            int(bottom),
            int(c.siblings),
            int(steps),
            int(json),
            sci(blake.cycles),
            sci(quot.cycles),
            sci(decode.cycles),
            sci(blake.plus(quot).plus(decode).cycles)
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

fn sci(v: f64) -> String {
    format!("{v:.3e}")
}
