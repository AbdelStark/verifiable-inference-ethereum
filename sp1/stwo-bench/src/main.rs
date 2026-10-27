//! Runs nitroo's Stwo verifier primitives without the `prover` feature in the SP1 guest, each in its
//! own cycle-tracker region: the Merkle hasher and the channel SASH's verifier uses
//! (`Blake2sM31MerkleChannel`, `Blake2sM31Channel`), and QM31 multiply-adds.
#![no_main]
sp1_zkvm::entrypoint!(main);

use std::hint::black_box;

use stwo::core::channel::{Blake2sM31Channel, Channel};
use stwo::core::fields::m31::BaseField;
use stwo::core::fields::qm31::SecureField;
use stwo::core::vcs::MerkleHasher;
use stwo::core::vcs::blake2_merkle::Blake2sM31MerkleHasher;

pub fn main() {
    let input = sp1_zkvm::io::read_vec();
    let n = u32::from_le_bytes(input.as_slice().try_into().expect("stdin holds one u32: N"));

    println!("cycle-tracker-report-start: hash_node");
    let leaf: Vec<BaseField> = (0..16).map(BaseField::from).collect();
    let mut node = Blake2sM31MerkleHasher::hash_node(None, black_box(&leaf));
    for i in 0..n {
        node = if i % 2 == 0 {
            Blake2sM31MerkleHasher::hash_node(Some((node, black_box(node))), &[])
        } else {
            Blake2sM31MerkleHasher::hash_node(None, black_box(&leaf))
        };
    }
    println!("cycle-tracker-report-end: hash_node");

    println!("cycle-tracker-report-start: channel_draw");
    let mut channel = Blake2sM31Channel::default();
    let mut drawn = SecureField::from(BaseField::from(0));
    for _ in 0..n {
        drawn += black_box(channel.draw_secure_felt());
    }
    println!("cycle-tracker-report-end: channel_draw");

    println!("cycle-tracker-report-start: qm31_mul_add");
    let mut acc = black_box(SecureField::from_u32_unchecked(1, 2, 3, 4));
    let factor = black_box(SecureField::from_u32_unchecked(5, 6, 7, 8));
    let addend = black_box(SecureField::from_u32_unchecked(9, 10, 11, 12));
    for _ in 0..n {
        acc = acc * factor + addend;
    }
    println!("cycle-tracker-report-end: qm31_mul_add");

    sp1_zkvm::io::commit_slice(&node.0);
    for felt in [drawn, acc] {
        for limb in felt.to_m31_array() {
            sp1_zkvm::io::commit_slice(&limb.0.to_le_bytes());
        }
    }
}
