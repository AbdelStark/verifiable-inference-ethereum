//! The synthetic corpus (`sp1/scripts/sash-synthetic.sh`), its committed records and the
//! mutations the bundle and differential tests share.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{env, fs};

use sash_evm_primitives::tinyllama_fixture::TINYLLAMA_TOK_IN;
use sash_segment_check::{P, Reject, SegmentPublicValues, check_segment};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zk_prover::common::Proof;

/// The corpus directory, from `SASH_SYNTHETIC_CORPUS`. Unset is a failure, never a skip. Its files
/// are checked against `sp1/testdata/synthetic.sha256` once per test binary.
pub fn corpus() -> &'static PathBuf {
    static CORPUS: OnceLock<PathBuf> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let dir = PathBuf::from(env::var_os("SASH_SYNTHETIC_CORPUS").expect(
            "SASH_SYNTHETIC_CORPUS must name the synthetic corpus (sp1/testdata/synthetic, \
             from sp1/scripts/sash-synthetic.sh or CI's sash-synthetic-corpus artifact)",
        ));
        let pins = fs::read_to_string(testdata().join("synthetic.sha256")).unwrap();
        for line in pins.lines() {
            let (sha, name) = line.split_once("  ").expect("sha256sum line");
            let bytes = fs::read(dir.join(name))
                .unwrap_or_else(|e| panic!("read {}: {e}", dir.join(name).display()));
            assert_eq!(
                hex::encode(Sha256::digest(&bytes)),
                sha,
                "{name} does not match sp1/testdata/synthetic.sha256"
            );
        }
        dir
    })
}

pub fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}

/// `sp1/testdata/synthetic.oracle.json`: upstream's verdict and binding table per bundle.
pub fn oracle_records() -> &'static BTreeMap<String, Value> {
    static RECORDS: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    RECORDS.get_or_init(|| {
        serde_json::from_slice(&fs::read(testdata().join("synthetic.oracle.json")).unwrap())
            .unwrap()
    })
}

pub const BUNDLES: [&str; 8] = [
    "sound_aligned",
    "aligned_gkr5",
    "sound_gkrfive",
    "skip_residual_stream",
    "skip_layer_boundary",
    "skip_kv_boundary",
    "skip_embedding",
    "head",
];

/// One bundle's three files, as JSON bytes.
#[derive(Clone)]
pub struct Bundle {
    pub fixed: Vec<u8>,
    pub claims: Vec<u8>,
    pub proof: Vec<u8>,
}

impl Bundle {
    pub fn load(tag: &str) -> Self {
        let read = |kind: &str| fs::read(corpus().join(format!("{tag}.{kind}.json"))).unwrap();
        Self {
            fixed: read("fixed"),
            claims: read("claims"),
            proof: read("proof"),
        }
    }

    pub fn fixed_json(&self) -> Value {
        serde_json::from_slice(&self.fixed).unwrap()
    }

    pub fn edit_fixed(&mut self, edit: impl FnOnce(&mut Value)) {
        let mut v = self.fixed_json();
        edit(&mut v);
        self.fixed = serde_json::to_vec(&v).unwrap();
    }

    pub fn edit_claims(&mut self, edit: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_slice(&self.claims).unwrap();
        edit(&mut v);
        self.claims = serde_json::to_vec(&v).unwrap();
    }

    pub fn edit_proof(&mut self, edit: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_slice(&self.proof).unwrap();
        edit(&mut v);
        self.proof = serde_json::to_vec(&v).unwrap();
    }

    /// The guest's proof buffer (`SPEC.md` §1.1): the parsed `StarkProof` as `bincode` 1.3.3.
    pub fn proof_bincode(&self) -> Vec<u8> {
        let proof: Proof = serde_json::from_slice(&self.proof).expect("proof.json parses");
        bincode::serialize(&proof).unwrap()
    }

    /// `check_segment` on the guest's three buffers. A panic is a rejection with no variant.
    pub fn check(&self) -> Outcome {
        let proof = self.proof_bincode();
        match panic::catch_unwind(AssertUnwindSafe(|| {
            check_segment(&self.fixed, &self.claims, &proof)
        })) {
            Ok(Ok(values)) => Outcome::Accept(Box::new(values)),
            Ok(Err(reject)) => Outcome::Reject(reject),
            Err(_) => Outcome::Panic,
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Accept(Box<SegmentPublicValues>),
    Reject(Reject),
    Panic,
}

impl Outcome {
    pub fn variant(&self) -> &'static str {
        match self {
            Outcome::Accept(_) => "accept",
            Outcome::Reject(r) => r.name(),
            Outcome::Panic => "panic",
        }
    }
}

/// A mutation of a positive bundle that stays structurally valid, and the variant
/// `check_segment` must return for it (`"accept"` when it must accept).
pub struct Mutation {
    pub name: &'static str,
    pub base: &'static str,
    pub apply: fn(&mut Bundle),
    pub expect: &'static str,
}

impl Mutation {
    pub fn bundle(&self) -> Bundle {
        let mut b = Bundle::load(self.base);
        (self.apply)(&mut b);
        b
    }
}

fn flip_hex_digit(v: &mut Value) {
    let s = v.as_str().unwrap();
    let last = if s.ends_with('0') { '1' } else { '0' };
    *v = json!(format!("{}{last}", &s[..s.len() - 1]));
}

pub const MUTATIONS: [Mutation; 19] = [
    Mutation {
        name: "merkle_hash_flipped",
        base: "sound_aligned",
        apply: |b| {
            b.edit_proof(|p| {
                let byte = &mut p["decommitments"][2]["hash_witness"][0][0];
                *byte = json!(byte.as_u64().unwrap() ^ 1);
            })
        },
        expect: "NotAccepted",
    },
    Mutation {
        name: "claims_value_changed",
        base: "sound_aligned",
        apply: |b| {
            b.edit_claims(|c| {
                let limb = &mut c["ao_out"][0];
                *limb = json!((limb.as_u64().unwrap() + 1) % u64::from(P));
            })
        },
        expect: "NotAccepted",
    },
    Mutation {
        name: "pre_root_changed",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| flip_hex_digit(&mut f["pre_root"])),
        expect: "NotAccepted",
    },
    Mutation {
        name: "model_root_changed",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| flip_hex_digit(&mut f["model_root"])),
        expect: "NotAccepted",
    },
    Mutation {
        name: "claims_truncated",
        base: "sound_aligned",
        apply: |b| b.claims.truncate(b.claims.len() / 2),
        expect: "NotAccepted",
    },
    Mutation {
        name: "log_blowup_drifted",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["log_blowup"] = json!(2)),
        expect: "SecurityParams",
    },
    Mutation {
        name: "n_queries_drifted",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["n_queries"] = json!(99)),
        expect: "SecurityParams",
    },
    Mutation {
        name: "pow_bits_drifted",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["pow_bits"] = json!(1)),
        expect: "SecurityParams",
    },
    Mutation {
        name: "tok_in_raised_by_p",
        base: "sound_aligned",
        apply: |b| {
            let mut ids = TINYLLAMA_TOK_IN;
            ids[8] += P;
            b.edit_fixed(|f| f["tok_in"] = json!(ids));
        },
        expect: "TokenIdOutOfField",
    },
    Mutation {
        name: "layer_removed",
        base: "sound_aligned",
        apply: |b| {
            b.edit_fixed(|f| {
                f.as_object_mut().unwrap().remove("layer");
            })
        },
        expect: "MissingLayer",
    },
    Mutation {
        name: "layer_65536",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["layer"] = json!(65536)),
        expect: "MissingLayer",
    },
    Mutation {
        name: "argmax_lax",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["params"]["prove_argmax_lax"] = json!(true)),
        expect: "UnboundEdge",
    },
    Mutation {
        // Edge 15: a layer-0 segment without the embedding gadget.
        name: "layer_1_to_0",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["layer"] = json!(0)),
        expect: "UnboundEdge",
    },
    Mutation {
        // Both halves of rule 5: `kmax` above `KMAX_SOUND`, and the `pow2` and `chain` Specs'
        // `2^(kmax + 9)` above `P`. The two below trip one half each.
        name: "kmax_22",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["params"]["kmax"] = json!(22)),
        expect: "UnsoundDequant",
    },
    Mutation {
        // Only `kmax <= KMAX_SOUND`: a `limbs` Spec is sound at any `kmax`.
        name: "limbs_kmax_22",
        base: "sound_aligned",
        apply: |b| {
            b.edit_fixed(|f| {
                for m in f["params"]["mats"].as_array_mut().unwrap() {
                    m["variant"] = json!("limbs");
                }
                f["params"]["kmax"] = json!(22);
            })
        },
        expect: "UnsoundDequant",
    },
    Mutation {
        // Only `Spec::sound()`, at `kmax` 16: a `fold` Spec with `q_bits + rem_bits = 7 + 24 > 30`.
        name: "down_fold_rem_bits_24",
        base: "sound_aligned",
        apply: |b| {
            b.edit_fixed(|f| {
                f["params"]["mats"][6]["variant"] = json!("fold");
                f["params"]["fold_rem_bits"] = json!(24);
            })
        },
        expect: "UnsoundDequant",
    },
    Mutation {
        name: "fixed_truncated",
        base: "sound_aligned",
        apply: |b| b.fixed.truncate(b.fixed.len() / 2),
        expect: "Malformed",
    },
    Mutation {
        // Accepted with another segmentIndex and fixedHash: the manifest's fixedHash pin (#47)
        // is what refuses it.
        name: "layer_1_to_2",
        base: "sound_aligned",
        apply: |b| b.edit_fixed(|f| f["layer"] = json!(2)),
        expect: "accept",
    },
    Mutation {
        name: "aligned_gkr5_pre_root_changed",
        base: "aligned_gkr5",
        apply: |b| b.edit_fixed(|f| flip_hex_digit(&mut f["pre_root"])),
        expect: "NotAccepted",
    },
];
