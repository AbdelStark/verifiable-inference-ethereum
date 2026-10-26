use std::env;
use std::process;

use sash_evm_primitives::grid::{compute_public_digest, verify_sash_grid};
use sash_evm_primitives::tinyllama_fixture::{
    tinyllama_manifest, tinyllama_segments, TINYLLAMA_ANSWER_IDS, TINYLLAMA_PROMPT,
    TINYLLAMA_PROMPT_IDS, TINYLLAMA_RESPONSE, TINYLLAMA_TOK_IN, TINYLLAMA_TOK_OUT,
};
use sha2::{Digest, Sha256};

fn main() {
    let cmd = env::args()
        .nth(1)
        .unwrap_or_else(|| "verify-fixture".to_string());
    match cmd.as_str() {
        "verify-fixture" => {
            let manifest = tinyllama_manifest();
            let segs = tinyllama_segments();
            verify_sash_grid(
                &manifest,
                &segs,
                &TINYLLAMA_TOK_IN,
                &TINYLLAMA_TOK_OUT,
                &TINYLLAMA_PROMPT_IDS,
                &TINYLLAMA_ANSWER_IDS,
            )
            .expect("TinyLlama-1.1B 23-segment fixture verification failed");

            let model_id: [u8; 32] = Sha256::digest(
                b"TinyLlama/TinyLlama-1.1B-Chat-v1.0@fe8a4ea1ffedaf415f4da2f062534de366a451e6:y-attnout-w12",
            )
            .into();
            let digest = compute_public_digest(model_id, &segs);
            println!("Verified 23-segment TinyLlama-1.1B SASH grid:");
            println!("  prompt:         {TINYLLAMA_PROMPT}");
            println!("  response:       {TINYLLAMA_RESPONSE}");
            println!("  model_id:       0x{}", hex::encode(model_id));
            println!(
                "  manifest_hash:  0x{}",
                hex::encode(manifest.manifest_roots_hash)
            );
            println!("  public_digest:  0x{}", hex::encode(digest));
        }
        other => {
            eprintln!("Unknown command '{other}'. Expected 'verify-fixture'.");
            process::exit(1);
        }
    }
}
