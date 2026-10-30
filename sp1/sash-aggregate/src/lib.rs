//! The aggregator guest's check (`SPEC.md` §1.2): the two input buffers' lengths, each segment's
//! public-values digest for `verify_sp1_proof`, and `publicDigest` (§1.3). Pure: the guest
//! (`sp1/aggregator-guest`) adds the `verify_sp1_proof` calls and the commitment.
#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use sash_evm_primitives::segment::{SEGMENT_CLAIM_LEN, SEGMENT_PUBLIC_VALUES_LEN};
use sha2::{Digest, Sha256};

/// Length of `modelId`.
pub const MODEL_ID_LEN: usize = 32;

/// `tokInHash`'s bytes in a segment's public values (`SPEC.md` §1.1).
const TOK_IN_HASH: core::ops::Range<usize> = SEGMENT_CLAIM_LEN..SEGMENT_CLAIM_LEN + 32;

/// `tokOutHash`'s bytes in a segment's public values (`SPEC.md` §1.1).
const TOK_OUT_HASH: core::ops::Range<usize> = SEGMENT_CLAIM_LEN + 32..SEGMENT_PUBLIC_VALUES_LEN;

/// An accepted input: what the guest verifies and commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aggregate {
    /// `publicDigest` (`SPEC.md` §1.3), the 32 bytes the guest commits.
    pub public_digest: [u8; 32],
    /// `sha256(values_i)` of each 226-byte chunk of `segmentValues`, in segment order: the
    /// public-values digests the guest passes to `verify_sp1_proof`.
    pub segment_digests: Vec<[u8; 32]>,
}

/// Why the aggregator rejects its input (`SPEC.md` §1.2, controls A1 and A2), in check order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregateReject {
    /// `modelId` is not 32 bytes.
    ModelIdLength { got: usize },
    /// `segmentValues` is empty, or its length is not a multiple of 226.
    SegmentValuesLength { got: usize },
}

impl AggregateReject {
    pub fn name(&self) -> &'static str {
        match self {
            AggregateReject::ModelIdLength { .. } => "ModelIdLength",
            AggregateReject::SegmentValuesLength { .. } => "SegmentValuesLength",
        }
    }
}

/// Checks the aggregator's two buffers and computes what it verifies and commits: `publicDigest =
/// sha256(modelId || be32(N) || claim(0) || ... || claim(N-1) || tokInHash || tokOutHash)`, with
/// `claim(i) = values_i[0..162]`, `tokInHash` from segment `0` and `tokOutHash` from segment `N - 1`.
/// Every other token-hash field is ignored (`SPEC.md` §1.2); `modelId` is hashed, never checked.
pub fn aggregate(model_id: &[u8], segment_values: &[u8]) -> Result<Aggregate, AggregateReject> {
    if model_id.len() != MODEL_ID_LEN {
        return Err(AggregateReject::ModelIdLength {
            got: model_id.len(),
        });
    }
    let (segments, rest) = segment_values.as_chunks::<SEGMENT_PUBLIC_VALUES_LEN>();
    if segments.is_empty() || !rest.is_empty() {
        return Err(AggregateReject::SegmentValuesLength {
            got: segment_values.len(),
        });
    }
    let n = u32::try_from(segments.len()).expect("N fits a u32: 226 * 2^32 bytes fit no guest");

    let mut h = Sha256::new();
    h.update(model_id);
    h.update(n.to_be_bytes());
    for values in segments {
        h.update(&values[..SEGMENT_CLAIM_LEN]);
    }
    h.update(&segments[0][TOK_IN_HASH]);
    h.update(&segments[segments.len() - 1][TOK_OUT_HASH]);
    Ok(Aggregate {
        public_digest: h.finalize().into(),
        segment_digests: segments.iter().map(|v| Sha256::digest(v).into()).collect(),
    })
}

#[cfg(test)]
mod tests;
