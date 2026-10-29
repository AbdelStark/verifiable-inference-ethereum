//! `sp1-host fixture-from-bundles`: `TinyLlamaFixture.sol` rendered from what the 23 segment guests
//! committed (`<tag>.public-values`, written by `sp1-host execute --public-values`), the token vectors
//! of `B0L0.fixed.json` and `head.fixed.json`, and what only enrollment fixes (`modelId`, `eosTokenId`,
//! the prompt and response text) from `tinyllama_fixture.rs`. The output never lands under
//! `contracts/src`: the committed fixture changes only through `make vectors` (AGENTS.md §2.4).

use std::fs;
use std::path::{Path, PathBuf};

use sash_evm_primitives::fixture::{assemble_fixture, render_fixture};
use sash_evm_primitives::segment::SegmentPublicValues;
use sash_evm_primitives::tinyllama_fixture::{
    TINYLLAMA_EOS_TOKEN_ID, TINYLLAMA_PROMPT, TINYLLAMA_RESPONSE, tinyllama_model_id,
};
use zk_prover::compose::Fixed;

use crate::tinyllama_tags;

/// The checkout's `contracts/src`, the tree the output never enters.
pub fn contracts_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/src")
}

/// `out` made absolute, if it is no symlink (`fs::write` follows one, even a dangling one, to a target
/// that may not exist yet) and its directory exists and is neither `contracts_src` nor under it once
/// both are canonicalized (so `../` and symlinked directories resolve).
pub fn check_fixture_out(out: &Path, contracts_src: &Path) -> Result<PathBuf, String> {
    let name = out
        .file_name()
        .ok_or(format!("--out {} names no file", out.display()))?;
    let parent = match out.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let dir = fs::canonicalize(parent)
        .map_err(|e| format!("--out's directory {}: {e}", parent.display()))?;
    let src =
        fs::canonicalize(contracts_src).map_err(|e| format!("{}: {e}", contracts_src.display()))?;
    let path = dir.join(name);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!(
            "--out {} is a symlink, which could lead under {}: the committed fixture changes only through `make vectors`",
            out.display(),
            src.display()
        ));
    }
    if dir.starts_with(&src) {
        return Err(format!(
            "--out {} is under {}: the committed fixture changes only through `make vectors`",
            out.display(),
            src.display()
        ));
    }
    Ok(path)
}

/// The fixture rendered from `<tag>.public-values` in `public_values`, for the 23 TinyLlama tags in
/// segment order, and `tok_in` / `tok_out` of `B0L0.fixed.json` / `head.fixed.json` in `bundles`.
pub fn fixture_from_bundles(
    public_values: &Path,
    bundles: &Path,
    prompt_len: usize,
) -> Result<String, String> {
    let values = tinyllama_tags()
        .iter()
        .map(|tag| {
            let path = public_values.join(format!("{tag}.public-values"));
            SegmentPublicValues::decode(&read(&path)?)
                .map_err(|e| format!("{}: {e:?}", path.display()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let tok_in = read_fixed(&bundles.join("B0L0.fixed.json"))?.tok_in;
    let tok_out = read_fixed(&bundles.join("head.fixed.json"))?.tok_out;
    let run = assemble_fixture(
        &values,
        &tok_in,
        &tok_out,
        prompt_len,
        TINYLLAMA_EOS_TOKEN_ID,
    )
    .map_err(|e| format!("the public values do not assemble: {e:?}"))?;
    Ok(render_fixture(&run.into_fixture(
        tinyllama_model_id(),
        TINYLLAMA_PROMPT.to_string(),
        TINYLLAMA_RESPONSE.to_string(),
    )))
}

/// A `<tag>.fixed.json`, parsed with the `Fixed` type the segment guest parses it with.
fn read_fixed(path: &Path) -> Result<Fixed, String> {
    serde_json::from_slice(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}
