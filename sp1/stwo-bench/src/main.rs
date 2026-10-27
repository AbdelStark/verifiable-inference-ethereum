//! Runs one verifier primitive of nitroo's Stwo (without the `prover` feature) `N` times in the SP1
//! guest, in its own cycle-tracker region, for the unit costs of `docs/EVM-GAS-MODEL.md` §6.
//!
//! Stdin: one 8-byte buffer `op: u32 LE || n: u32 LE`, then, for the decode operations only, the
//! `StarkProof` to decode. Every input is `black_box`ed, so the loops run in full. Each operation
//! commits a value derived from its result, so none of the work is dead.
#![no_main]
sp1_zkvm::entrypoint!(main);

use std::hint::black_box;

use blake2::Blake2sVarCore;
use blake2::digest::core_api::{Buffer, UpdateCore, VariableOutputCore};
use stwo::core::channel::{Blake2sM31Channel, Channel};
use stwo::core::circle::{CirclePoint, M31_CIRCLE_GEN};
use stwo::core::fields::m31::BaseField;
use stwo::core::fields::qm31::SecureField;
use stwo::core::lookups::gkr_verifier::{GkrBatchProof, partially_verify_batch};
use stwo::core::pcs::quotients::{
    ColumnSampleBatch, PointSample, accumulate_row_quotients, quotient_constants,
};
use stwo::core::proof::StarkProof;
use stwo::core::vcs::MerkleHasher;
use stwo::core::vcs::blake2_merkle::Blake2sM31MerkleHasher;

/// SASH's proof type (`Proof` in `prover/src/common.rs`).
type Proof = StarkProof<Blake2sM31MerkleHasher>;

/// The quotient row: `QUOTIENT_COLUMNS` columns sampled at one point, the first
/// `QUOTIENT_TWO_SAMPLE_COLUMNS` of them at a second point too: one (column, sample) step each.
const QUOTIENT_COLUMNS: usize = 1024;
const QUOTIENT_TWO_SAMPLE_COLUMNS: usize = 64;

/// Never taken by the tests: it only proves the GKR verifier, moved out of nitroo's `prover` feature
/// by sp1/patches/0002, links into a guest. Real GKR proofs run in the segment guest.
const OP_GKR_LINK: u32 = u32::MAX;

pub fn main() {
    let header = sp1_zkvm::io::read_vec();
    let header: [u8; 8] = header
        .as_slice()
        .try_into()
        .expect("stdin holds op: u32, n: u32");
    let op = u32::from_le_bytes(header[..4].try_into().unwrap());
    let n = u32::from_le_bytes(header[4..].try_into().unwrap());
    match op {
        0 => blake2s_compress(n),
        1 => hash_node_leaf::<4>("hash_node_leaf4", n),
        2 => hash_node_leaf::<16>("hash_node_leaf16", n),
        3 => hash_node_leaf::<20>("hash_node_leaf20", n),
        4 => hash_node_leaf::<256>("hash_node_leaf256", n),
        5 => hash_node_leaf::<4096>("hash_node_leaf4096", n),
        6 => hash_node_children(n),
        7 => channel_draw(n),
        8 => qm31_loop("qm31_add", n, |acc, x, _| acc + x),
        9 => qm31_loop("qm31_mul", n, |acc, x, _| acc * x),
        10 => qm31_loop("qm31_mul_add", n, |acc, x, y| acc * x + y),
        11 => m31_qm31_mul(n),
        12 => row_quotients(n),
        13 => decode("decode_json", n, |b| {
            serde_json::from_slice::<Proof>(b).ok()
        }),
        14 => decode("decode_bincode", n, |b| {
            bincode::deserialize::<Proof>(b).ok()
        }),
        15 => decode("decode_postcard", n, |b| {
            postcard::from_bytes::<Proof>(b).ok()
        }),
        OP_GKR_LINK => gkr_link(),
        _ => panic!("unknown op {op}"),
    }
}

fn start(region: &str) {
    println!("cycle-tracker-report-start: {region}");
}

fn end(region: &str) {
    println!("cycle-tracker-report-end: {region}");
}

/// One BLAKE2s compression (`blake2` 0.10.6's core `update_blocks` on one 64-byte block).
fn blake2s_compress(n: u32) {
    let block = [7u8; 64].into();
    let mut core = Blake2sVarCore::new_with_params(&[], &[], 0, 32);
    start("blake2s_compress");
    for _ in 0..n {
        core.update_blocks(std::slice::from_ref(black_box(&block)));
    }
    end("blake2s_compress");
    let mut out = Default::default();
    core.finalize_variable_core(&mut Buffer::<Blake2sVarCore>::default(), &mut out);
    sp1_zkvm::io::commit_slice(&out);
}

/// `hash_node` of SASH's Merkle hasher on a leaf of `M` M31 values and no children. The sizes 4 and 20
/// end in a partial 64-byte block, 16, 256 and 4096 in a whole one.
fn hash_node_leaf<const M: usize>(region: &str, n: u32) {
    let leaf: Vec<BaseField> = (0..M as u32).map(BaseField::from).collect();
    let mut acc = [0u8; 32];
    start(region);
    for _ in 0..n {
        let hash = Blake2sM31MerkleHasher::hash_node(None, black_box(&leaf));
        acc[0] ^= hash.0[0];
    }
    end(region);
    sp1_zkvm::io::commit_slice(&acc);
}

/// `hash_node` of SASH's Merkle hasher on two children and no column values.
fn hash_node_children(n: u32) {
    let leaf: Vec<BaseField> = (0..16).map(BaseField::from).collect();
    let sibling = Blake2sM31MerkleHasher::hash_node(None, &leaf);
    let mut node = sibling;
    start("hash_node_children");
    for _ in 0..n {
        node = Blake2sM31MerkleHasher::hash_node(Some((node, black_box(sibling))), &[]);
    }
    end("hash_node_children");
    sp1_zkvm::io::commit_slice(&node.0);
}

/// `draw_secure_felt` of the channel SASH's verifier uses.
fn channel_draw(n: u32) {
    let mut channel = Blake2sM31Channel::default();
    let mut drawn = SecureField::from(BaseField::from(0));
    start("channel_draw");
    for _ in 0..n {
        drawn += black_box(channel.draw_secure_felt());
    }
    end("channel_draw");
    commit_secure(drawn);
}

fn qm31_loop(
    region: &str,
    n: u32,
    f: impl Fn(SecureField, SecureField, SecureField) -> SecureField,
) {
    let mut acc = black_box(SecureField::from_u32_unchecked(1, 2, 3, 4));
    let x = SecureField::from_u32_unchecked(5, 6, 7, 8);
    let y = SecureField::from_u32_unchecked(9, 10, 11, 12);
    start(region);
    for _ in 0..n {
        acc = f(acc, black_box(x), black_box(y));
    }
    end(region);
    commit_secure(acc);
}

/// `M31 * QM31`, the product `accumulate_row_quotients` takes per (column, sample).
fn m31_qm31_mul(n: u32) {
    let mut acc = black_box(SecureField::from_u32_unchecked(1, 2, 3, 4));
    let x = BaseField::from(123_456_789);
    start("m31_qm31_mul");
    for _ in 0..n {
        acc = black_box(x) * acc;
    }
    end("m31_qm31_mul");
    commit_secure(acc);
}

/// `accumulate_row_quotients` on one row of the shape above, as `fri_answers` calls it once per query
/// position. Commits the row's (column, sample) step count, which the host divides by.
fn row_quotients(n: u32) {
    let z = CirclePoint::<SecureField>::get_point(98_765_432_123);
    let z_prev = CirclePoint::<SecureField>::get_point(12_345_678_987);
    let samples: Vec<Vec<(PointSample, SecureField)>> = (0..QUOTIENT_COLUMNS)
        .map(|i| {
            let sample = |point, k: u32| {
                let value = SecureField::from_u32_unchecked(i as u32, k, 3, 4);
                let coeff = SecureField::from_u32_unchecked(k, i as u32, 5, 6);
                (PointSample { point, value }, coeff)
            };
            if i < QUOTIENT_TWO_SAMPLE_COLUMNS {
                vec![sample(z_prev, 1), sample(z, 2)]
            } else {
                vec![sample(z, 2)]
            }
        })
        .collect();
    let batches = ColumnSampleBatch::new_vec(&samples.iter().collect::<Vec<_>>());
    let steps: usize = batches.iter().map(|b| b.cols_vals_randpows.len()).sum();
    let constants = quotient_constants(&batches);
    let row: Vec<BaseField> = (0..QUOTIENT_COLUMNS as u32)
        .map(|i| BaseField::from(i * 7919))
        .collect();
    let domain_point = M31_CIRCLE_GEN.mul(1 << 20);
    let mut acc = SecureField::from(BaseField::from(0));
    start("row_quotients");
    for _ in 0..n {
        acc += accumulate_row_quotients(
            black_box(&batches),
            black_box(&row),
            black_box(&constants),
            black_box(domain_point),
        );
    }
    end("row_quotients");
    sp1_zkvm::io::commit_slice(&(steps as u32).to_le_bytes());
    commit_secure(acc);
}

/// Decodes the stdin buffer as a `StarkProof` `n` times. Commits how many decodes succeeded, then the
/// last decoded proof's commitment count and queried-value count, for the host to check. An empty or
/// undecodable buffer still runs the region.
fn decode(region: &str, n: u32, f: impl Fn(&[u8]) -> Option<Proof>) {
    let buf = sp1_zkvm::io::read_vec();
    let mut ok = 0u32;
    let mut last = None;
    start(region);
    for _ in 0..n {
        if let Some(proof) = f(black_box(&buf)) {
            ok += 1;
            last = Some(black_box(proof));
        }
    }
    end(region);
    let (commitments, queried) = last.map_or((0, 0), |p| {
        let queried = p.0.queried_values.iter().map(Vec::len).sum::<usize>();
        (p.0.commitments.len(), queried)
    });
    for word in [ok, commitments as u32, queried as u32] {
        sp1_zkvm::io::commit_slice(&word.to_le_bytes());
    }
}

fn gkr_link() {
    let empty = GkrBatchProof {
        sumcheck_proofs: Vec::new(),
        layer_masks_by_instance: Vec::new(),
        output_claims_by_instance: Vec::new(),
    };
    let verified = partially_verify_batch(Vec::new(), &empty, &mut Blake2sM31Channel::default());
    sp1_zkvm::io::commit_slice(&[verified.is_ok() as u8]);
}

fn commit_secure(felt: SecureField) {
    for limb in felt.to_m31_array() {
        sp1_zkvm::io::commit_slice(&limb.0.to_le_bytes());
    }
}
