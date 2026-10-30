//! What the proving commands check on the CPU before any GPU work: the guests' Docker-pinned ELFs and
//! vkeys, the TinyLlama segment tags, each segment's expected public values (`SPEC.md` §1.1, §1.4) and
//! a segment's compressed proof file.

use std::fs;
use std::path::{Path, PathBuf};

use bincode::Options;
use sash_evm_primitives::fixture::tinyllama_segment_public_values;
use sash_evm_primitives::segment::SEGMENT_PUBLIC_VALUES_LEN;
use sha2::{Digest, Sha256};
use sp1_sdk::blocking::LightProver;
use sp1_sdk::blocking::prelude::*;
use sp1_sdk::{SP1Proof, SP1VerifyingKey};

use crate::tinyllama_tags;

/// A guest's Docker-built ELF sha256 and vkey, as committed in `sp1/<name>.elf.sha256` and
/// `sp1/<name>.vkey`.
pub struct GuestPins {
    pub name: &'static str,
    /// `sp1/<name>.elf.sha256`, in `sha256sum` format.
    pub elf_sha256: &'static str,
    /// `sp1/<name>.vkey`: `vk.bytes32()` and `vk.hash_u32()`, as [`vkey_text`] prints them.
    pub vkey: &'static str,
}

/// The segment guest (`SPEC.md` §1.1).
pub const SEGMENT_GUEST: GuestPins = GuestPins {
    name: "segment-guest",
    elf_sha256: include_str!("../../segment-guest.elf.sha256"),
    vkey: include_str!("../../segment-guest.vkey"),
};

/// The aggregator guest (`SPEC.md` §1.2).
pub const AGGREGATOR_GUEST: GuestPins = GuestPins {
    name: "aggregator-guest",
    elf_sha256: include_str!("../../aggregator-guest.elf.sha256"),
    vkey: include_str!("../../aggregator-guest.vkey"),
};

impl GuestPins {
    /// Rejects an ELF whose sha256 is not the pinned one.
    pub fn check_elf(&self, elf: &[u8]) -> Result<(), String> {
        let pinned = self
            .elf_sha256
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let actual = hex::encode(Sha256::digest(elf));
        if actual != pinned {
            return Err(format!(
                "the ELF's sha256 {actual} differs from sp1/{}.elf.sha256 ({pinned}); pass the Docker-built ELF",
                self.name
            ));
        }
        Ok(())
    }

    /// Rejects a vkey (as [`vkey_text`] prints it) that differs from the pinned one.
    pub fn check_vkey(&self, text: &str) -> Result<(), String> {
        if text != self.vkey {
            return Err(format!(
                "the ELF's vkey\n{text}differs from sp1/{}.vkey\n{}",
                self.name, self.vkey
            ));
        }
        Ok(())
    }

    /// The pinned vkey's `bytes32` (`vk.bytes32()`), as `sp1/<name>.vkey` records it.
    pub fn bytes32(&self) -> Result<&'static str, String> {
        self.vkey
            .lines()
            .find_map(|line| line.strip_prefix("bytes32: "))
            .ok_or(format!("no bytes32 line in sp1/{}.vkey", self.name))
    }

    /// Reads the ELF at `path` and checks its sha256, then its vkey from `light`'s CPU setup. Returns
    /// the ELF and its verifying key.
    pub fn load(&self, path: &Path, light: &LightProver) -> Result<(Elf, SP1VerifyingKey), String> {
        let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.check_elf(&bytes)?;
        let elf = Elf::from(bytes);
        let pk = light.setup(elf.clone()).map_err(|e| e.to_string())?;
        let vk = pk.verifying_key().clone();
        self.check_vkey(&vkey_text(&vk))?;
        println!(
            "OK: {} matches sp1/{}.elf.sha256 and, from a CPU setup, sp1/{}.vkey",
            path.display(),
            self.name,
            self.name
        );
        Ok((elf, vk))
    }
}

/// A vkey as `sp1-host vkey` prints it and `sp1/<guest>.vkey` records it.
pub fn vkey_text(vk: &SP1VerifyingKey) -> String {
    format!("bytes32: {}\nhash_u32: {:?}\n", vk.bytes32(), vk.hash_u32())
}

/// The repository's root: the checkout this binary was built from.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `--tags`: `all` (the 23 TinyLlama tags in segment order), or a comma-separated list of TinyLlama
/// tags, in the order given. Rejects an empty list and an unknown tag; repeats are the caller's to
/// rule on.
pub fn parse_tags(spec: &str) -> Result<Vec<String>, String> {
    if spec == "all" {
        return Ok(tinyllama_tags());
    }
    let known = tinyllama_tags();
    let tags: Vec<String> = spec.split(',').map(str::to_string).collect();
    for tag in &tags {
        if !known.contains(tag) {
            return Err(format!(
                "--tags: unknown tag {tag:?}; the TinyLlama tags are B0L0..B0L21 and head, or all"
            ));
        }
    }
    Ok(tags)
}

/// The first tag that occurs twice in `tags`, if any.
pub fn repeated_tag(tags: &[String]) -> Option<&str> {
    tags.iter()
        .enumerate()
        .find(|(i, t)| tags[..*i].contains(t))
        .map(|(_, t)| t.as_str())
}

/// The public values the segment guest commits on `tag`'s bundle (`tinyllama_segment_public_values`,
/// encoded from `tinyllama_fixture.rs`): what its proof must carry, byte for byte.
pub fn expected_public_values(tag: &str) -> Result<[u8; SEGMENT_PUBLIC_VALUES_LEN], String> {
    let index = tinyllama_tags()
        .iter()
        .position(|t| t == tag)
        .ok_or(format!("unknown tag {tag:?}"))?;
    Ok(tinyllama_segment_public_values()[index])
}

/// A segment's `<tag>.proof.bin`, decoded, before its verification: bincode
/// `SP1ProofWithPublicValues` (`SP1ProofWithPublicValues::save`'s encoding, the whole file and nothing
/// after it), an `SP1Proof::Compressed`, whose public values are `expected` byte for byte.
pub fn decode_segment_proof(
    bytes: &[u8],
    expected: &[u8],
) -> Result<SP1ProofWithPublicValues, String> {
    let proof = decode_proof(bytes)?;
    if proof.public_values.as_slice() != expected {
        return Err(format!(
            "public values {} differ from the expected {}",
            hex::encode(proof.public_values.as_slice()),
            hex::encode(expected)
        ));
    }
    if !matches!(proof.proof, SP1Proof::Compressed(_)) {
        return Err(format!("an SP1Proof::{}, not Compressed", proof.proof));
    }
    Ok(proof)
}

/// A proof file, decoded: bincode `SP1ProofWithPublicValues` (`SP1ProofWithPublicValues::save`'s
/// encoding, the whole file and nothing after it).
pub fn decode_proof(bytes: &[u8]) -> Result<SP1ProofWithPublicValues, String> {
    bincode::options()
        .with_fixint_encoding()
        .with_limit(bytes.len() as u64)
        .reject_trailing_bytes()
        .deserialize(bytes)
        .map_err(|e| format!("not a bincode SP1ProofWithPublicValues: {e}"))
}

/// Verifies a decoded segment proof on the CPU against the segment guest's `vk`.
pub fn verify_on_cpu(
    light: &LightProver,
    proof: &SP1ProofWithPublicValues,
    vk: &SP1VerifyingKey,
) -> Result<(), String> {
    light
        .verify(proof, vk, None)
        .map_err(|e| format!("does not verify against the segment guest's vk: {e}"))
}

/// Appends `proof`'s compressed inner proof and `vk` to `stdin` (`SP1Stdin::write_proof`): the
/// proof SP1's deferred-proof recursion checks against one `verify_sp1_proof` call (`SPEC.md` §1.2).
pub fn write_proof(
    stdin: &mut SP1Stdin,
    proof: &SP1ProofWithPublicValues,
    vk: &SP1VerifyingKey,
) -> Result<(), String> {
    match &proof.proof {
        SP1Proof::Compressed(inner) => {
            stdin.write_proof((**inner).clone(), vk.vk.clone());
            Ok(())
        }
        other => Err(format!("an SP1Proof::{other}, not Compressed")),
    }
}
