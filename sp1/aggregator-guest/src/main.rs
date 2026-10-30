//! SP1 aggregator guest (`SPEC.md` §1.2): reads `modelId` and the `N` segments' public values, runs
//! `aggregate`, defers one `verify_sp1_proof` per segment under `SEGMENT_VKEY` and commits the 32
//! bytes of `publicDigest`. A rejection panics, which exits non-zero.
#![no_main]
sp1_zkvm::entrypoint!(main);

use sash_aggregate::aggregate;
use sp1_zkvm::lib::verify::verify_sp1_proof;

/// The segment guest's vkey, `hash_u32` in `sp1/segment-guest.vkey`; never read from the input.
/// `sp1-host`'s `segment_vkey_is_the_pinned_one` test fails unless the two are equal.
const SEGMENT_VKEY: [u32; 8] = [
    498718820, 1712855298, 516113602, 160047780, 1060073053, 876725599, 1114556383, 281319804,
];

pub fn main() {
    // Raw bytes, never `io::read`: its bincode decode halts with exit code 3 on bad bytes, which
    // must instead reach `aggregate` and reject with their variant.
    let model_id = sp1_zkvm::io::read_vec();
    let segment_values = sp1_zkvm::io::read_vec();
    let aggregate = match aggregate(&model_id, &segment_values) {
        Ok(aggregate) => aggregate,
        Err(reject) => {
            // The details first, in one write: the host keeps only the last 2 KiB of stderr,
            // which the short panic message, the variant's name, always ends.
            let details = format!("{reject:?}\n");
            eprint!("{details}");
            panic!("aggregate rejected: {}", reject.name());
        }
    };
    // Checked only by the deferred-proof recursion when proving, never by the executor (SPEC.md §1.2).
    for digest in &aggregate.segment_digests {
        verify_sp1_proof(&SEGMENT_VKEY, digest);
    }
    sp1_zkvm::io::commit_slice(&aggregate.public_digest);
}
