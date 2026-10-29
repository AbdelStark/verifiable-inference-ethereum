//! `sp1-host prove`'s checks (`docs/EVM-GAS-MODEL.md` §8): the ELF and its vkey are the committed
//! program's, the `fixedHash` its pin, and the proof file lands under `runs/` or outside the checkout,
//! never in the bundle directory. None of them needs a GPU.

use std::fs;
use std::path::{Path, PathBuf};

use sash_evm_primitives::segment::SegmentPublicValues;
use sha2::{Digest, Sha256};
use sp1_sdk::SP1VerifyingKey;
use sp1_sdk::blocking::prelude::*;

use crate::parse_fixed_pins;

/// `sp1/segment-guest.elf.sha256`: the sha256 of #43's Docker-built segment guest.
pub const ELF_SHA256: &str = include_str!("../../segment-guest.elf.sha256");

/// `sp1/segment-guest.vkey`: that ELF's `vk.bytes32()` and `vk.hash_u32()`, as `sp1-host vkey` prints them.
pub const VKEY: &str = include_str!("../../segment-guest.vkey");

/// `sp1/pins/tinyllama.fixed.sha256`: each TinyLlama segment's `fixedHash` (#45).
pub const FIXED_PINS: &str = include_str!("../../pins/tinyllama.fixed.sha256");

/// The repository's root: the checkout this binary was built from.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Rejects an ELF whose sha256 is not the one in `sp1/segment-guest.elf.sha256`.
pub fn check_elf(elf: &[u8]) -> Result<(), String> {
    let pinned = ELF_SHA256.split_whitespace().next().unwrap_or_default();
    let actual = hex::encode(Sha256::digest(elf));
    if actual != pinned {
        return Err(format!(
            "the ELF's sha256 {actual} differs from sp1/segment-guest.elf.sha256 ({pinned}); pass #43's Docker-built ELF"
        ));
    }
    Ok(())
}

/// A vkey as `sp1-host vkey` prints it and `sp1/segment-guest.vkey` records it.
pub fn vkey_text(vk: &SP1VerifyingKey) -> String {
    format!("bytes32: {}\nhash_u32: {:?}\n", vk.bytes32(), vk.hash_u32())
}

/// Rejects a vkey (as [`vkey_text`]) that differs from `sp1/segment-guest.vkey`.
pub fn check_vkey(text: &str) -> Result<(), String> {
    if text != VKEY {
        return Err(format!(
            "the ELF's vkey\n{text}differs from sp1/segment-guest.vkey\n{VKEY}"
        ));
    }
    Ok(())
}

/// `tag`'s pin in `pins` (the text of `sp1/pins/tinyllama.fixed.sha256`).
pub fn fixed_pin(pins: &str, tag: &str) -> Result<[u8; 32], String> {
    parse_fixed_pins(pins, "sp1/pins/tinyllama.fixed.sha256")
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|(t, _)| t == tag)
        .map(|(_, hash)| hash)
        .ok_or(format!("no fixedHash pin for {tag}"))
}

/// The `fixedHash` of the 226 committed bytes, if it equals `pin`.
pub fn check_fixed_hash(public_values: &[u8], pin: &[u8; 32]) -> Result<[u8; 32], String> {
    let values =
        SegmentPublicValues::decode(public_values).map_err(|e| format!("public values: {e:?}"))?;
    if &values.fixed_hash != pin {
        return Err(format!(
            "fixedHash {} differs from its pin {}",
            hex::encode(values.fixed_hash),
            hex::encode(pin)
        ));
    }
    Ok(values.fixed_hash)
}

/// The proof file's path, made absolute: named `<tag>.proof.bin`, not an existing path (writing
/// follows a symlink, even a dangling one, to a target the directory checks never see), in an
/// existing directory that is `<root>/runs/` or under it, or outside `root`, and never `bundle` or
/// inside it. Proof bytes never land in the checkout's tracked tree or beside the pinned bundle
/// files. [`create_out`] opens the path at the end and refuses whatever appeared there meanwhile.
pub fn check_out(out: &Path, tag: &str, root: &Path, bundle: &Path) -> Result<PathBuf, String> {
    let name = format!("{tag}.proof.bin");
    if out.file_name().and_then(|n| n.to_str()) != Some(name.as_str()) {
        return Err(format!("--out {} must be named {name}", out.display()));
    }
    let parent = match out.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let dir = fs::canonicalize(parent)
        .map_err(|e| format!("--out's directory {}: {e}", parent.display()))?;
    let root = fs::canonicalize(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let path = dir.join(name);
    match fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() => {
            return Err(format!(
                "--out {} is a symlink, which could lead into the checkout or the bundle directory",
                out.display()
            ));
        }
        Ok(_) => return Err(format!("--out {} already exists", out.display())),
        Err(_) => {}
    }
    if dir.starts_with(&root) && !dir.starts_with(root.join("runs")) {
        return Err(format!(
            "--out {} is inside the checkout {} and not under its runs/",
            out.display(),
            root.display()
        ));
    }
    // A bundle directory that does not exist cannot hold the existing `dir`.
    if let Ok(bundle) = fs::canonicalize(bundle)
        && dir.starts_with(&bundle)
    {
        return Err(format!(
            "--out {} is in the bundle directory {}",
            out.display(),
            bundle.display()
        ));
    }
    Ok(path)
}

/// Creates the proof file at `path`, which [`check_out`] returned: `create_new`, so it refuses any
/// existing path, a symlink or a dangling one included, that appeared there since the check.
pub fn create_out(path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// `VmHWM` of this process' `sp1-gpu-server` child (`/proc/<pid>/status`), which runs the prover:
/// this process' own RSS misses it. `None` without `/proc` or without such a child.
pub fn gpu_server_vm_hwm() -> Option<String> {
    let me = std::process::id().to_string();
    fs::read_dir("/proc").ok()?.flatten().find_map(|entry| {
        let status = fs::read_to_string(entry.path().join("status")).ok()?;
        let field = |name: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(name))
                .map(|v| v.trim().to_string())
        };
        (field("Name:")? == "sp1-gpu-server" && field("PPid:")? == me)
            .then(|| field("VmHWM:"))
            .flatten()
    })
}
