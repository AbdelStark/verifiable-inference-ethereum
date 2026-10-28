//! Reference SASH 23-segment grid checks and settlement digests for Ethereum EVM settlement,
//! mirroring the Solidity verifier bit for bit.

// Host-only: the segment guest (`target_os = "zkvm"`) links this crate, and a module more in its
// build reorders the guest's code, so its ELF and vkey would no longer be the committed ones.
#[cfg(not(target_os = "zkvm"))]
pub mod fixture;
pub mod grid;
pub mod segment;
pub mod tinyllama_fixture;
pub mod token;
