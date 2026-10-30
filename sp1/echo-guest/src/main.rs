//! Control-only SP1 guest for binding control B2 (`SPEC.md` §2.1): commits its one input buffer
//! unchanged, so that a proof of it can carry a segment's exact public values under a vkey other
//! than `SEGMENT_VKEY`. Used only by `sp1-host aggregate-control --kind vk`; not Docker-pinned.
#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    sp1_zkvm::io::commit_slice(&sp1_zkvm::io::read_vec());
}
