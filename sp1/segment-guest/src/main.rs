//! SP1 segment guest. This first version checks token ids against the field (`SPEC.md` §1.1, rule 1)
//! and commits their `tokenHash` (§1.3); the full segment check replaces its body.
#![no_main]
sp1_zkvm::entrypoint!(main);

use sash_evm_primitives::grid::compute_token_hash;

/// The M31 modulus `P = 2^31 - 1`. SASH maps token ids to M31 unreduced, so an id `P + k` would alias `k`.
const P: u32 = (1 << 31) - 1;

pub fn main() {
    // Raw bytes, not `io::read`: a bincode decode failure there halts with exit code 3, not a panic.
    let buf = sp1_zkvm::io::read_vec();
    let (chunks, rest) = buf.as_chunks::<4>();
    assert!(
        rest.is_empty(),
        "token buffer of {} bytes is not a whole number of u32 ids",
        buf.len()
    );
    let ids: Vec<u32> = chunks
        .iter()
        .map(|chunk| u32::from_le_bytes(*chunk))
        .collect();
    for (index, &id) in ids.iter().enumerate() {
        assert!(
            id < P,
            "TokenIdOutOfField: id {id} at index {index} is not below P = 2^31 - 1"
        );
    }
    sp1_zkvm::io::commit_slice(&compute_token_hash(&ids));
}
