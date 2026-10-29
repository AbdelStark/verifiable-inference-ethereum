//! Deterministically generates the Solidity TinyLlama-1.1B 23-segment fixture library for Foundry
//! from `tinyllama_fixture.rs` (`make vectors`).

use std::env;
use std::fs;
use std::process;

use sash_evm_primitives::fixture::render_fixture;
use sash_evm_primitives::tinyllama_fixture::tinyllama_fixture_data;

fn main() {
    let Some(out_path) = env::args().nth(1) else {
        eprintln!("usage: export_vectors <output .sol path>");
        process::exit(2);
    };
    fs::write(out_path, render_fixture(&tinyllama_fixture_data())).expect("write Solidity fixture");
}
