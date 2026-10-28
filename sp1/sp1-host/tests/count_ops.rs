//! The BLAKE2s compression counter of `sp1/patches/0005` (`count-ops`), which `native-check` reads:
//! one count per 64-byte block a hash absorbs, the last one possibly partial, and one for the empty
//! input, per hash, never per `update` call; `hash_node` and the channel apart.
//! `cargo test --locked -p sp1-host --features profiling,count-ops`.
#![cfg(feature = "count-ops")]

use stwo::core::channel::{Blake2sM31Channel, Channel};
use stwo::core::fields::m31::BaseField;
use stwo::core::vcs::MerkleHasher;
use stwo::core::vcs::blake2_merkle::Blake2sM31MerkleHasher;
use stwo::core::vcs::count_ops::{counts, reset_counts};

fn hash_node_compressions(children: bool, m31s: u32) -> u64 {
    let leaf: Vec<BaseField> = (0..m31s).map(BaseField::from).collect();
    let child = Blake2sM31MerkleHasher::hash_node(None, &[]);
    reset_counts();
    Blake2sM31MerkleHasher::hash_node(children.then_some((child, child)), &leaf);
    let (hash_node, channel) = counts();
    assert_eq!(channel, 0);
    hash_node
}

// One test: the counters are process-wide.
#[test]
fn counts_compressions_per_hash() {
    // `hash_node` updates once per 4-byte M31; the count follows the bytes, not the calls.
    for (children, m31s, want) in [
        (false, 0, 1),
        (false, 1, 1),
        (false, 16, 1),
        (false, 17, 2),
        (false, 4096, 256),
        (true, 0, 1),
        (true, 1, 2),
        (true, 16, 2),
    ] {
        assert_eq!(
            hash_node_compressions(children, m31s),
            want,
            "children {children}, {m31s} M31s"
        );
    }
    // A channel draw hashes digest (32 B) + counter (4 B) + 1 B: one compression, counted apart.
    let mut channel = Blake2sM31Channel::default();
    reset_counts();
    channel.draw_u32s();
    assert_eq!(counts(), (0, 1));
}
