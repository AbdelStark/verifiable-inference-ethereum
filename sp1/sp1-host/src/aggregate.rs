//! The aggregator guest's host side (`SPEC.md` §1.2): its stdin without the segment proofs, its
//! executor run, and the segment vkey it hardcodes, read from its source so that a host test can
//! compare it with `sp1/segment-guest.vkey`.

use std::path::Path;

use sash_evm_primitives::segment::SEGMENT_PUBLIC_VALUES_LEN;
use sp1_sdk::SP1Stdin;
use sp1_sdk::blocking::prelude::*;

use crate::{Execution, run_executor, tinyllama_tags};

/// The aggregator guest's source, whose `SEGMENT_VKEY` literal [`segment_vkey`] reads.
pub const AGGREGATOR_GUEST_SOURCE: &str = include_str!("../../aggregator-guest/src/main.rs");

/// The aggregator's stdin without proofs: `modelId`, then `segmentValues`, each with `write_vec`.
pub fn aggregator_stdin(model_id: &[u8], segment_values: &[u8]) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(model_id.to_vec());
    stdin.write_vec(segment_values.to_vec());
    stdin
}

/// Runs the aggregator on `stdin` in the executor with deferred proof verification off, so that it
/// runs without the segment proofs: the executor treats `verify_sp1_proof` as a no-op either way
/// (`SPEC.md` §1.2).
pub fn execute_aggregator(elf: Elf, stdin: SP1Stdin) -> Result<Execution, String> {
    run_executor(elf, stdin, false)
}

/// The 23 TinyLlama `<tag>.public-values` in `dir` (as `sp1-host execute --public-values` writes
/// them), concatenated in segment order: the aggregator's `segmentValues`.
pub fn read_tinyllama_segment_values(dir: &Path) -> Result<Vec<u8>, String> {
    let mut values = Vec::with_capacity(23 * SEGMENT_PUBLIC_VALUES_LEN);
    for tag in tinyllama_tags() {
        let path = dir.join(format!("{tag}.public-values"));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() != SEGMENT_PUBLIC_VALUES_LEN {
            return Err(format!(
                "{}: {} bytes, not {SEGMENT_PUBLIC_VALUES_LEN}",
                path.display(),
                bytes.len()
            ));
        }
        values.extend_from_slice(&bytes);
    }
    Ok(values)
}

/// The `SEGMENT_VKEY` literal of the aggregator guest's `source`: the eight words between the
/// brackets after `const SEGMENT_VKEY: [u32; 8] = `.
pub fn segment_vkey(source: &str) -> Result<[u32; 8], String> {
    let decl = "const SEGMENT_VKEY: [u32; 8] = [";
    let start = source
        .find(decl)
        .ok_or(format!("no `{decl}` in the aggregator guest"))?
        + decl.len();
    let end = source[start..]
        .find(']')
        .ok_or("SEGMENT_VKEY has no closing bracket")?;
    parse_words(&source[start..start + end]).map_err(|e| format!("SEGMENT_VKEY: {e}"))
}

/// `hash_u32` of a vkey as `sp1-host vkey` prints it and `sp1/segment-guest.vkey` records it.
pub fn vkey_hash_u32(vkey: &str) -> Result<[u32; 8], String> {
    let words = vkey
        .lines()
        .find_map(|line| line.strip_prefix("hash_u32: ["))
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or("no `hash_u32: [...]` line")?;
    parse_words(words).map_err(|e| format!("hash_u32: {e}"))
}

/// Eight comma-separated `u32`s, with any whitespace and an optional trailing comma.
fn parse_words(list: &str) -> Result<[u32; 8], String> {
    let words = list
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(|w| w.parse::<u32>().map_err(|e| format!("{w:?}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    <[u32; 8]>::try_from(words).map_err(|w| format!("{} words, not 8", w.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_literal_and_the_pin_format() {
        let source =
            "/// x\nconst SEGMENT_VKEY: [u32; 8] = [\n    1, 2, 3,\n    4, 5, 6, 7, 8,\n];\n";
        assert_eq!(segment_vkey(source), Ok([1, 2, 3, 4, 5, 6, 7, 8]));
        let pin = "bytes32: 0x00\nhash_u32: [1, 2, 3, 4, 5, 6, 7, 4294967295]\n";
        assert_eq!(vkey_hash_u32(pin), Ok([1, 2, 3, 4, 5, 6, 7, u32::MAX]));
    }

    #[test]
    fn rejects_another_word_count_or_a_non_number() {
        assert!(segment_vkey("const SEGMENT_VKEY: [u32; 8] = [1, 2, 3, 4, 5, 6, 7];").is_err());
        assert!(vkey_hash_u32("hash_u32: [1, 2, 3, 4, 5, 6, 7, 8, 9]").is_err());
        assert!(vkey_hash_u32("hash_u32: [1, 2, 3, 4, 5, 6, 7, -8]").is_err());
        assert!(vkey_hash_u32("bytes32: 0x00\n").is_err());
    }
}
