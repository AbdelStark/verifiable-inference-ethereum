//! The proof-size rows of `docs/EVM-GAS-MODEL.md` §1, recomputed from a bundle directory
//! (`sp1-host sizes --bundles DIR`).

use std::fs;
use std::io;
use std::path::Path;

use zk_prover::common::Proof;
use zk_prover::compose::{Claims, Fixed};

/// One segment's §1 counts, or their sum over segments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    /// Committed Merkle trees: the proof's commitments.
    pub trees: u64,
    /// Committed trace columns: `Fixed.sizes`, every tree but the composition tree (8 columns).
    pub columns: u64,
    /// The largest column log size in `Fixed.sizes` (a maximum, not a sum, over segments).
    pub max_log_size: u32,
    /// FRI inner layers.
    pub fri_inner_layers: u64,
    /// `<tag>.fixed.json` + `<tag>.claims.json` + `<tag>.proof.json`.
    pub json_bytes: u64,
    /// stwo's `StarkProof::size_estimate()`: in memory, not a wire format.
    pub stark_size_estimate: u64,
    /// The GKR proof: `Claims.gkr`, upstream's `serde_json` bytes of it.
    pub gkr_bytes: u64,
    /// Out-of-domain samples: `QM31` values in `sampled_values`.
    pub sampled: u64,
    /// The committed trees' leaf `M31`s in the proof: queried values and column witness. The FRI
    /// layers' witness (`QM31`s) is not counted.
    pub leaf_m31s: u64,
    /// Sibling hashes: `hash_witness` of the trees' and the FRI layers' decommitments.
    pub siblings: u64,
}

impl Sizes {
    /// The counts of `<tag>.{fixed,claims,proof}.json` in `dir`.
    pub fn of_bundle(dir: &Path, tag: &str) -> io::Result<Self> {
        let mut json_bytes = 0;
        let mut read = |kind: &str| {
            let path = dir.join(format!("{tag}.{kind}.json"));
            let bytes = fs::read(&path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
            json_bytes += bytes.len() as u64;
            Ok::<_, io::Error>(bytes)
        };
        let fixed: Fixed = serde_json::from_slice(&read("fixed")?).map_err(io::Error::other)?;
        let claims: Claims = serde_json::from_slice(&read("claims")?).map_err(io::Error::other)?;
        let proof: Proof = serde_json::from_slice(&read("proof")?).map_err(io::Error::other)?;
        Ok(Self::of(&fixed, &claims, &proof, json_bytes))
    }

    pub fn of(fixed: &Fixed, claims: &Claims, proof: &Proof, json_bytes: u64) -> Self {
        let p = &proof.0;
        let fri = &p.fri_proof;
        let fri_layers = || std::iter::once(&fri.first_layer).chain(&fri.inner_layers);
        let len = |n: usize| n as u64;
        Self {
            trees: len(p.commitments.len()),
            columns: fixed.sizes.iter().map(|t| len(t.len())).sum(),
            max_log_size: fixed.sizes.iter().flatten().copied().max().unwrap_or(0),
            fri_inner_layers: len(fri.inner_layers.len()),
            json_bytes,
            stark_size_estimate: len(proof.size_estimate()),
            gkr_bytes: claims.gkr.as_ref().map_or(0, |g| len(g.len())),
            sampled: p
                .sampled_values
                .iter()
                .flatten()
                .map(|c| len(c.len()))
                .sum(),
            leaf_m31s: p.queried_values.iter().map(|t| len(t.len())).sum::<u64>()
                + p.decommitments
                    .iter()
                    .map(|d| len(d.column_witness.len()))
                    .sum::<u64>(),
            siblings: p
                .decommitments
                .iter()
                .map(|d| len(d.hash_witness.len()))
                .sum::<u64>()
                + fri_layers()
                    .map(|l| len(l.decommitment.hash_witness.len()))
                    .sum::<u64>(),
        }
    }

    /// The sum of two segments' counts; `max_log_size` is the larger.
    pub fn plus(&self, o: &Self) -> Self {
        Self {
            trees: self.trees + o.trees,
            columns: self.columns + o.columns,
            max_log_size: self.max_log_size.max(o.max_log_size),
            fri_inner_layers: self.fri_inner_layers + o.fri_inner_layers,
            json_bytes: self.json_bytes + o.json_bytes,
            stark_size_estimate: self.stark_size_estimate + o.stark_size_estimate,
            gkr_bytes: self.gkr_bytes + o.gkr_bytes,
            sampled: self.sampled + o.sampled,
            leaf_m31s: self.leaf_m31s + o.leaf_m31s,
            siblings: self.siblings + o.siblings,
        }
    }
}

/// `n` with thousands separators.
pub fn int(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One row of §1's table: its name and its cell for one column.
type Row = (&'static str, fn(&Sizes) -> String);

/// §1's table: one column per `columns` tag, then the sum over `all` (tag, counts) segments.
pub fn table(all: &[(String, Sizes)], columns: &[&str]) -> String {
    let total = all.iter().fold(Sizes::default(), |acc, (_, s)| acc.plus(s));
    let picked: Vec<&Sizes> = columns
        .iter()
        .map(|c| &all.iter().find(|(t, _)| t == c).expect("column tag").1)
        .chain([&total])
        .collect();
    let mut out = String::from("| Quantity |");
    for c in columns {
        out.push_str(&format!(" `{c}` |"));
    }
    out.push_str(&format!(" All {} segments |\n|---|", all.len()));
    out.push_str(&"---:|".repeat(columns.len() + 1));
    out.push('\n');
    let rows: [Row; 10] = [
        ("Committed Merkle trees", |s| int(s.trees)),
        ("Committed trace columns", |s| int(s.columns)),
        ("Max trace `log_size`", |s| s.max_log_size.to_string()),
        ("FRI inner layers", |s| int(s.fri_inner_layers)),
        ("Raw JSON bundle size", |s| {
            format!("{} B", int(s.json_bytes))
        }),
        (
            "STARK proof, stwo `size_estimate()` (in memory, not a wire format)",
            |s| format!("{} B", int(s.stark_size_estimate)),
        ),
        ("GKR proof, `serde_json` bytes", |s| {
            format!("{} B", int(s.gkr_bytes))
        }),
        ("Sampled OODS `QM31` values", |s| int(s.sampled)),
        ("Queried + witness leaf `M31`s", |s| int(s.leaf_m31s)),
        ("Internal Merkle + FRI sibling hashes", |s| int(s.siblings)),
    ];
    for (name, cell) in rows {
        out.push_str(&format!("| {name} |"));
        for s in &picked {
            out.push_str(&format!(" {} |", cell(s)));
        }
        out.push('\n');
    }
    out
}
