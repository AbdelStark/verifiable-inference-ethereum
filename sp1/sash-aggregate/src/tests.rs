//! `aggregate` against `SPEC.md` §1.2-§1.4 and `grid.rs`, and controls A1 and A2 (§2.1), natively.
//! The executor runs the same in `sp1-host/tests/aggregator_guest.rs`.

use alloc::vec;
use alloc::vec::Vec;

use sash_evm_primitives::fixture::tinyllama_segment_public_values;
use sash_evm_primitives::grid::compute_public_digest;
use sash_evm_primitives::tinyllama_fixture::{
    TINYLLAMA_TOK_IN, TINYLLAMA_TOK_OUT, tinyllama_model_id, tinyllama_segments,
};
use sha2::{Digest, Sha256};

use super::*;

/// `SPEC.md` §1.4's `publicDigest`.
const SPEC_PUBLIC_DIGEST: &str = "cf52e19e82f82680f0098ab435b52e22512f3d634bd7a947d32abc76d067b3ef";

/// `B0L10`, segment 10 of the TinyLlama run.
const B0L10: usize = 10;

fn segment_values() -> Vec<u8> {
    tinyllama_segment_public_values().concat()
}

#[test]
fn tinyllama_digest_is_the_grid_digest_and_spec_value() {
    let agg = aggregate(&tinyllama_model_id(), &segment_values()).expect("accepts");
    let grid = compute_public_digest(
        tinyllama_model_id(),
        &tinyllama_segments(),
        &TINYLLAMA_TOK_IN,
        &TINYLLAMA_TOK_OUT,
    );
    assert_eq!(agg.public_digest, grid);
    assert_eq!(hex::encode(agg.public_digest), SPEC_PUBLIC_DIGEST);
}

#[test]
fn segment_digests_are_each_chunk_sha256_in_order() {
    let values = tinyllama_segment_public_values();
    let agg = aggregate(&tinyllama_model_id(), &values.concat()).expect("accepts");
    let expected: Vec<[u8; 32]> = values.iter().map(|v| Sha256::digest(v).into()).collect();
    assert_eq!(agg.segment_digests.len(), 23);
    assert_eq!(agg.segment_digests, expected);
}

/// `SPEC.md` §1.2: the token-hash fields of segments `1..21`, `tokOutHash` of segment 0 and `tokInHash`
/// of `head` are ignored.
#[test]
fn other_token_hash_fields_do_not_change_the_digest() {
    let mut values = tinyllama_segment_public_values();
    for v in &mut values[1..22] {
        v[162..226].fill(0xa5);
    }
    values[0][194..226].fill(0x5a);
    values[22][162..194].fill(0x5a);
    let agg = aggregate(&tinyllama_model_id(), &values.concat()).expect("accepts");
    assert_eq!(hex::encode(agg.public_digest), SPEC_PUBLIC_DIGEST);
}

/// `tokInHash` comes from segment 0 and `tokOutHash` from segment `N - 1`: changing either changes the
/// digest.
#[test]
fn first_tok_in_hash_and_last_tok_out_hash_are_hashed() {
    for (segment, byte) in [(0, 162), (0, 193), (22, 194), (22, 225)] {
        let mut values = tinyllama_segment_public_values();
        values[segment][byte] ^= 0x01;
        let agg = aggregate(&tinyllama_model_id(), &values.concat()).expect("accepts");
        assert_ne!(
            hex::encode(agg.public_digest),
            SPEC_PUBLIC_DIGEST,
            "segment {segment} byte {byte}"
        );
    }
}

/// Every claim byte is hashed: one flipped bit in any of them changes the digest.
#[test]
fn every_claim_byte_is_hashed() {
    for byte in 0..SEGMENT_CLAIM_LEN {
        let mut values = tinyllama_segment_public_values();
        values[B0L10][byte] ^= 0x80;
        let agg = aggregate(&tinyllama_model_id(), &values.concat()).expect("accepts");
        assert_ne!(
            hex::encode(agg.public_digest),
            SPEC_PUBLIC_DIGEST,
            "byte {byte}"
        );
    }
}

/// `N = 1`: `B0L10`'s values alone give `sha256(modelId || be32(1) || claim || 0^32 || 0^32)`.
#[test]
fn b0l10_alone() {
    let b0l10 = tinyllama_segment_public_values()[B0L10];
    let agg = aggregate(&tinyllama_model_id(), &b0l10).expect("accepts");
    let mut preimage = tinyllama_model_id().to_vec();
    preimage.extend_from_slice(&1u32.to_be_bytes());
    preimage.extend_from_slice(&b0l10[..162]);
    preimage.extend_from_slice(&[0; 64]);
    assert_eq!(preimage.len(), 100 + 162);
    assert_eq!(
        agg.public_digest,
        <[u8; 32]>::from(Sha256::digest(&preimage))
    );
    assert_eq!(
        agg.segment_digests,
        vec![<[u8; 32]>::from(Sha256::digest(b0l10))]
    );
}

/// A1 (`SPEC.md` §2.1): `modelId` of 31 or 33 bytes.
#[test]
fn a1_model_id_length() {
    let model_id = tinyllama_model_id();
    let mut long = model_id.to_vec();
    long.push(0);
    for (id, got) in [(&model_id[..31], 31), (&long[..], 33)] {
        let reject = aggregate(id, &segment_values()).unwrap_err();
        assert_eq!(reject, AggregateReject::ModelIdLength { got });
        assert_eq!(reject.name(), "ModelIdLength");
    }
}

/// A2 (`SPEC.md` §2.1): `segmentValues` empty, or `226 * 23 - 1` bytes.
#[test]
fn a2_segment_values_length() {
    let values = segment_values();
    for (bytes, got) in [(&[][..], 0), (&values[..226 * 23 - 1], 226 * 23 - 1)] {
        let reject = aggregate(&tinyllama_model_id(), bytes).unwrap_err();
        assert_eq!(reject, AggregateReject::SegmentValuesLength { got });
        assert_eq!(reject.name(), "SegmentValuesLength");
    }
}

/// `modelId` is checked first (`SPEC.md` §1.2's order).
#[test]
fn model_id_is_checked_before_segment_values() {
    let reject = aggregate(&[0; 31], &[]).unwrap_err();
    assert_eq!(reject, AggregateReject::ModelIdLength { got: 31 });
}
