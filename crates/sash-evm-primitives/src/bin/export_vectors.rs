//! Deterministically exports the 23-segment TinyLlama-1.1B SASH test vector JSON for Foundry.

use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use sha2::{Digest, Sha256};

use sash_evm_primitives::blake2s::{blake2s256, blake2s_m31};
use sash_evm_primitives::channel::derive_sash_w_residual_stream;
use sash_evm_primitives::grid::compute_public_digest;
use sash_evm_primitives::tinyllama_fixture::{
    tinyllama_manifest, tinyllama_segments, TINYLLAMA_ANSWER_IDS, TINYLLAMA_B0_REL_DRAWS,
    TINYLLAMA_EOS_TOKEN_ID, TINYLLAMA_HEAD_REL_DRAWS, TINYLLAMA_PROMPT, TINYLLAMA_PROMPT_IDS,
    TINYLLAMA_RESPONSE, TINYLLAMA_TOK_IN, TINYLLAMA_TOK_OUT,
};

#[derive(Serialize)]
struct HexSegment {
    segment_index: u16,
    relation_draws: u16,
    pre_root: String,
    model_root: String,
    trace_root: String,
    residual_in_root: String,
    residual_out_root: String,
    token_horner_claim: [u32; 4],
}

#[derive(Serialize)]
struct ExportFixture {
    schema: &'static str,
    model_id: String,
    prompt: &'static str,
    response: &'static str,
    num_segments: u16,
    seq_len: u16,
    eos_token_id: u32,
    b0_rel_draws: u16,
    head_rel_draws: u16,
    manifest_roots_hash: String,
    public_digest: String,
    blake2s_a_hex: String,
    blake2s_m31_a_hex: String,
    b0_w_residual_stream: [u32; 4],
    head_w_residual_stream: [u32; 4],
    prompt_ids: Vec<u32>,
    answer_ids: Vec<u32>,
    tok_in: Vec<u32>,
    tok_out: Vec<u32>,
    segments: Vec<HexSegment>,
}

fn hex0x(bytes: [u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn main() {
    let out_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("contracts/test/vectors/tinyllama_23seg_fixture.json"));

    let model_id: [u8; 32] =
        Sha256::digest(b"TinyLlama/TinyLlama-1.1B-Chat-v1.0@fe8a4ea1ffedaf415f4da2f062534de366a451e6:y-attnout-w12").into();
    let manifest = tinyllama_manifest();
    let segs = tinyllama_segments();
    let public_digest = compute_public_digest(model_id, &segs);

    let b0_w = derive_sash_w_residual_stream(
        segs[0].pre_root,
        segs[0].model_root,
        segs[0].trace_root,
        segs[0].relation_draws,
    );
    let head_w = derive_sash_w_residual_stream(
        segs[22].pre_root,
        segs[22].model_root,
        segs[22].trace_root,
        segs[22].relation_draws,
    );

    let fixture = ExportFixture {
        schema: "verifiable-inference-ethereum.tinyllama-fixture/1",
        model_id: hex0x(model_id),
        prompt: TINYLLAMA_PROMPT,
        response: TINYLLAMA_RESPONSE,
        num_segments: manifest.num_segments,
        seq_len: manifest.seq_len,
        eos_token_id: TINYLLAMA_EOS_TOKEN_ID,
        b0_rel_draws: TINYLLAMA_B0_REL_DRAWS,
        head_rel_draws: TINYLLAMA_HEAD_REL_DRAWS,
        manifest_roots_hash: hex0x(manifest.manifest_roots_hash),
        public_digest: hex0x(public_digest),
        blake2s_a_hex: hex0x(blake2s256(b"a")),
        blake2s_m31_a_hex: hex0x(blake2s_m31(b"a")),
        b0_w_residual_stream: b0_w.to_u32_array(),
        head_w_residual_stream: head_w.to_u32_array(),
        prompt_ids: TINYLLAMA_PROMPT_IDS.to_vec(),
        answer_ids: TINYLLAMA_ANSWER_IDS.to_vec(),
        tok_in: TINYLLAMA_TOK_IN.to_vec(),
        tok_out: TINYLLAMA_TOK_OUT.to_vec(),
        segments: segs
            .into_iter()
            .map(|s| HexSegment {
                segment_index: s.segment_index,
                relation_draws: s.relation_draws,
                pre_root: hex0x(s.pre_root),
                model_root: hex0x(s.model_root),
                trace_root: hex0x(s.trace_root),
                residual_in_root: hex0x(s.residual_in_root),
                residual_out_root: hex0x(s.residual_out_root),
                token_horner_claim: s.token_horner_claim.to_u32_array(),
            })
            .collect(),
    };

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).expect("create vector output directory");
    }
    let rendered = serde_json::to_string_pretty(&fixture).expect("serialize fixture") + "\n";
    fs::write(&out_path, rendered).expect("write vector json");
}
