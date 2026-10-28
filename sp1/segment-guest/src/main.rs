//! SP1 segment guest (`SPEC.md` §1.1): reads the segment's three buffers, runs `check_segment` and
//! commits its 226 bytes of public values. A rejection panics, which exits 1.
#![no_main]
sp1_zkvm::entrypoint!(main);

use sash_segment_check::check_segment;

pub fn main() {
    // Raw bytes, never `io::read`: its bincode decode halts with exit code 3 on bad bytes, which
    // must instead reach `check_segment` and reject with their variant.
    let fixed = sp1_zkvm::io::read_vec();
    let claims = sp1_zkvm::io::read_vec();
    let proof = sp1_zkvm::io::read_vec();
    match check_segment(&fixed, &claims, &proof) {
        Ok(values) => sp1_zkvm::io::commit_slice(&values.encode()),
        Err(reject) => {
            // The details first, in one write: the host keeps only the last 2 KiB of stderr,
            // which the short panic message, the variant's name, always ends.
            let details = format!("{reject:?}\n");
            eprint!("{details}");
            panic!("segment rejected: {}", reject.name());
        }
    }
}
