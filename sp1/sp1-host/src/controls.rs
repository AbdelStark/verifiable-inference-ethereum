//! The controls on the real TinyLlama bundles (`make sp1-execute`, `docs/EVM-GAS-MODEL.md` §7): each
//! writes a modified copy of one segment's three files to its own directory, never the bundle
//! directory, and runs the segment check on it natively and the segment guest in the executor.

use std::fs;
use std::path::Path;

use sash_segment_check::check_segment;
use sash_segment_check::forge::{Vector, forge};
use serde_json::{Value, json};
use sp1_sdk::blocking::prelude::*;
use zk_prover::common::Proof;
use zk_prover::compose::{Claims, Fixed};

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

/// The unmodified segment, parsed, for the edits that derive their change from it.
pub struct Original {
    pub fixed: Fixed,
    pub claims: Claims,
    pub proof: Proof,
}

pub struct Control {
    pub name: &'static str,
    pub tag: &'static str,
    pub file: File,
    pub what: &'static str,
    pub edit: fn(&mut Value, &Original),
    pub expect: Expect,
}

fn plus_one_mod_p(v: &mut Value) {
    *v = json!((v.as_u64().expect("an M31 value") + 1) % P);
}

fn plus_p(v: &mut Value) {
    *v = json!(v.as_u64().expect("a token id") + P);
}

/// G9: `tok_in[25]` (`B0L0`) or `tok_out[24]` (`head`) set to 450, with four slack positions solved
/// so that the Horner sum under the `w` the segment was proved with is unchanged
/// (`sash_segment_check::forge`). Panics if neither transcript reproduces the claims.
fn forge_vector(f: &mut Value, original: &Original, vector: Vector) {
    let forged = forge(&original.fixed, &original.claims, &original.proof, vector)
        .unwrap_or_else(|e| panic!("G9 forgery of {}: {e}", vector.key()));
    eprintln!(
        "G9 forgery of {}: solved under the {:?} transcript's w",
        vector.key(),
        forged.transcript
    );
    f[vector.key()] = json!(forged.ids);
}

pub const CONTROLS: [Control; 9] = [
    Control {
        name: "proof_queried_value",
        tag: "B0L10",
        file: File::Proof,
        what: "`queried_values[0][0]` + 1 mod P (the proof still decodes)",
        edit: |p, _| plus_one_mod_p(&mut p["queried_values"][0][0]),
        expect: Expect::Reject("NotAccepted"),
    },
    Control {
        name: "claims_value",
        tag: "B0L10",
        file: File::Claims,
        what: "`ao_out[0]` + 1 mod P (still decodable)",
        edit: |c, _| plus_one_mod_p(&mut c["ao_out"][0]),
        expect: Expect::Reject("NotAccepted"),
    },
    Control {
        name: "tok_in_plus_p",
        tag: "B0L0",
        file: File::Fixed,
        what: "`tok_in[0]` + P",
        edit: |f, _| plus_p(&mut f["tok_in"][0]),
        expect: Expect::Reject("TokenIdOutOfField"),
    },
    Control {
        name: "tok_out_plus_p",
        tag: "head",
        file: File::Fixed,
        what: "`tok_out[0]` + P",
        edit: |f, _| plus_p(&mut f["tok_out"][0]),
        expect: Expect::Reject("TokenIdOutOfField"),
    },
    Control {
        name: "tok_in_horner_forged",
        tag: "B0L0",
        file: File::Fixed,
        what: "`tok_in[25]` = 450, `tok_in[1..=4]` solved to keep the Horner sum under the segment's own `w`",
        edit: |f, o| forge_vector(f, o, Vector::TokIn),
        expect: Expect::Reject("NotAccepted"),
    },
    Control {
        name: "tok_out_horner_forged",
        tag: "head",
        file: File::Fixed,
        what: "`tok_out[24]` = 450, `tok_out[0..=3]` solved to keep the Horner sum under the segment's own `w`",
        edit: |f, o| forge_vector(f, o, Vector::TokOut),
        expect: Expect::Reject("NotAccepted"),
    },
    Control {
        name: "block_removed",
        tag: "B0L10",
        file: File::Fixed,
        what: "`block` removed (`nn_pos_ok` holds without it)",
        edit: |f, _| {
            f.as_object_mut().expect("Fixed").remove("block");
        },
        expect: Expect::Reject("NotSingleBlock"),
    },
    Control {
        name: "head_block_1",
        tag: "head",
        file: File::Fixed,
        what: "`block` 0 to 1 (the head has no rotation, so `nn_pos_ok` holds)",
        edit: |f, _| f["block"] = json!(1),
        expect: Expect::Reject("NotSingleBlock"),
    },
    Control {
        name: "fixed_model_changed",
        tag: "B0L10",
        file: File::Fixed,
        what: "`model`'s last hex digit changed (no rule reads it)",
        edit: |f, _| {
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
        let files = [File::Fixed, File::Claims, File::Proof].map(|file| {
            let name = format!("{}.{}.json", self.tag, file.kind());
            let bytes = fs::read(bundles.join(&name)).map_err(|e| format!("{name}: {e}"));
            (file, name, bytes)
        });
        let parse = |i: usize| match &files[i].2 {
            Ok(bytes) => Ok(bytes.as_slice()),
            Err(e) => Err(e.clone()),
        };
        let original = Original {
            fixed: serde_json::from_slice(parse(0)?).map_err(|e| format!("fixed.json: {e}"))?,
            claims: serde_json::from_slice(parse(1)?).map_err(|e| format!("claims.json: {e}"))?,
            proof: serde_json::from_slice(parse(2)?).map_err(|e| format!("proof.json: {e}"))?,
        };
        for (file, name, bytes) in files {
            let bytes = bytes?;
            let bytes = if file.kind() == self.file.kind() {
                let mut v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                (self.edit)(&mut v, &original);
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
