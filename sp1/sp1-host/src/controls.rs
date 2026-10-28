//! The controls on the real TinyLlama bundles (`make sp1-execute`, `docs/EVM-GAS-MODEL.md` §7): each
//! writes a modified copy of one segment's three files to its own directory, never the bundle
//! directory, and runs the segment check on it natively and the segment guest in the executor.

use std::fs;
use std::path::Path;

use sash_segment_check::check_segment;
use serde_json::{Value, json};
use sp1_sdk::blocking::prelude::*;

use crate::{Execution, SegmentInput, execute};

/// `P = 2^31 - 1`, the M31 modulus (`SPEC.md` §1.1 rule 1).
const P: u64 = (1 << 31) - 1;

/// The bytes of `fixedHash` in the 226 public-value bytes (`SPEC.md` §1.1).
pub const FIXED_HASH: std::ops::Range<usize> = 130..162;

/// What a control must do in the guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// Exit 1, with a panic message naming this `Reject` variant.
    Reject(&'static str),
    /// Exit 0 with the unmodified segment's public values but for `fixedHash`, which differs from
    /// the segment's pin. #47's `manifestRootsHash` check is what rejects it.
    FixedHashDiffers,
}

/// Which of the segment's three files a control edits.
#[derive(Clone, Copy, Debug)]
pub enum File {
    Fixed,
    Claims,
    Proof,
}

impl File {
    pub fn kind(self) -> &'static str {
        match self {
            File::Fixed => "fixed",
            File::Claims => "claims",
            File::Proof => "proof",
        }
    }
}

pub struct Control {
    pub name: &'static str,
    pub tag: &'static str,
    pub file: File,
    pub what: &'static str,
    pub edit: fn(&mut Value),
    pub expect: Expect,
}

fn plus_one_mod_p(v: &mut Value) {
    *v = json!((v.as_u64().expect("an M31 value") + 1) % P);
}

fn plus_p(v: &mut Value) {
    *v = json!(v.as_u64().expect("a token id") + P);
}

pub const CONTROLS: [Control; 5] = [
    Control {
        name: "proof_queried_value",
        tag: "B0L10",
        file: File::Proof,
        what: "`queried_values[0][0]` + 1 mod P (the proof still decodes)",
        edit: |p| plus_one_mod_p(&mut p["queried_values"][0][0]),
        expect: Expect::Reject("NotAccepted"),
    },
    Control {
        name: "claims_value",
        tag: "B0L10",
        file: File::Claims,
        what: "`ao_out[0]` + 1 mod P (still decodable)",
        edit: |c| plus_one_mod_p(&mut c["ao_out"][0]),
        expect: Expect::Reject("NotAccepted"),
    },
    Control {
        name: "tok_in_plus_p",
        tag: "B0L0",
        file: File::Fixed,
        what: "`tok_in[0]` + P",
        edit: |f| plus_p(&mut f["tok_in"][0]),
        expect: Expect::Reject("TokenIdOutOfField"),
    },
    Control {
        name: "tok_out_plus_p",
        tag: "head",
        file: File::Fixed,
        what: "`tok_out[0]` + P",
        edit: |f| plus_p(&mut f["tok_out"][0]),
        expect: Expect::Reject("TokenIdOutOfField"),
    },
    Control {
        name: "fixed_model_changed",
        tag: "B0L10",
        file: File::Fixed,
        what: "`model`'s last hex digit changed (no rule reads it)",
        edit: |f| {
            let model = f["model"].as_str().expect("Fixed.model").to_string();
            let last = if model.ends_with('0') { '1' } else { '0' };
            f["model"] = json!(format!("{}{last}", &model[..model.len() - 1]));
        },
        expect: Expect::FixedHashDiffers,
    },
];

impl Control {
    /// Writes the modified copy of the segment's three files to `dir`.
    pub fn write_copy(&self, bundles: &Path, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for file in [File::Fixed, File::Claims, File::Proof] {
            let name = format!("{}.{}.json", self.tag, file.kind());
            let bytes = fs::read(bundles.join(&name)).map_err(|e| format!("{name}: {e}"))?;
            let bytes = if file.kind() == self.file.kind() {
                let mut v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                (self.edit)(&mut v);
                serde_json::to_vec(&v).map_err(|e| e.to_string())?
            } else {
                bytes
            };
            fs::write(dir.join(&name), bytes).map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }

    /// Runs the segment check natively and the guest `elf` in the executor on the copy in `dir`, and
    /// checks both outcomes against `expect`, given the unmodified segment's public values and
    /// `fixedHash` pin. Returns the executor run and a one-line result.
    pub fn run(
        &self,
        elf: Elf,
        dir: &Path,
        original: &[u8],
        pin: &[u8; 32],
    ) -> Result<(Execution, String), String> {
        let input = SegmentInput::read_bundle(dir, self.tag).map_err(|e| e.to_string())?;
        let native = check_segment(&input.fixed, &input.claims, &input.proof);
        let run = execute(elf, input.stdin())?;
        let result = match self.expect {
            Expect::Reject(variant) => {
                if native.as_ref().err().map(|r| r.name()) != Some(variant) {
                    return Err(format!(
                        "{}: expected {variant} natively, got {native:?}",
                        self.name
                    ));
                }
                if run.exit_code != 1 || run.reject() != Some(variant) {
                    return Err(format!(
                        "{}: expected exit 1 with {variant}, got exit {} with {:?}\n{}",
                        self.name,
                        run.exit_code,
                        run.reject(),
                        run.stderr
                    ));
                }
                format!("native {variant}; executor exit 1, segment rejected: {variant}")
            }
            Expect::FixedHashDiffers => {
                let pv = &run.public_values;
                let ok = native.as_ref().is_ok_and(|v| v.encode()[..] == pv[..])
                    && run.exit_code == 0
                    && pv.len() == original.len()
                    && pv[..FIXED_HASH.start] == original[..FIXED_HASH.start]
                    && pv[FIXED_HASH.end..] == original[FIXED_HASH.end..]
                    && pv[FIXED_HASH] != pin[..];
                if !ok {
                    return Err(format!(
                        "{}: expected exit 0 with only fixedHash changed, equal to native, got exit {}, public values {}, native {native:?}\n{}",
                        self.name,
                        run.exit_code,
                        hex::encode(pv),
                        run.stderr
                    ));
                }
                format!(
                    "native and executor accept with equal public values; exit 0, fixedHash {} != pin {}",
                    hex::encode(&pv[FIXED_HASH]),
                    hex::encode(pin)
                )
            }
        };
        Ok((run, result))
    }
}
