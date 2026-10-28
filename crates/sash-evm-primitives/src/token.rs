//! `tokenHash` (`SPEC.md` §1.3), apart from the grid checks: the segment guest links it, so a
//! change to `grid.rs` leaves the guest's ELF and vkey unchanged.

use sha2::{Digest, Sha256};

/// `sha256` over token ids, each encoded as a 32-byte big-endian word
/// (Solidity `sha256(abi.encodePacked(uint32[]))`).
pub fn compute_token_hash(ids: &[u32]) -> [u8; 32] {
    let mut h = Sha256::new();
    for id in ids {
        h.update([0u8; 28]);
        h.update(id.to_be_bytes());
    }
    h.finalize().into()
}
