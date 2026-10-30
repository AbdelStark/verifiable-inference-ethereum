#!/usr/bin/env bash
# make sp1-proofs-check and the sp1-bundles CI job: the guard against vkey drift. Checks the 24 proof
# files in PROOFS (sp1/scripts/tinyllama-proofs.sh, make proofs-download) against
# sp1/pins/tinyllama.proofs.sha256, then runs `sp1-host aggregate --tags all --execute-only` on the 23
# segment proofs with the Docker-built ELFs: both ELFs against sp1/<guest>.elf.sha256 and, from a CPU
# setup, sp1/<guest>.vkey; the segment vk's hash_u32 against the aggregator's SEGMENT_VKEY; each proof
# decoded, its public values the expected ones, verified on the CPU against the segment vk; the digest,
# SPEC.md §1.4's publicDigest; and the aggregator in the executor with the 23 proofs in stdin, deferred
# proof verification on, committing it. No GPU. A guest-linked change that moves either guest's pins
# fails here until the segments are re-proved and the pins and contracts/fixtures/tinyllama.plonk.json
# re-committed (AGENTS.md §5), which is paid GPU work that needs approval (AGENTS.md §2.6).
#
# PROOFS: the directory tinyllama-proofs.sh fills. ELF and AGGREGATOR_ELF: the segment and aggregator
# guests from `make sp1-docker`, or the `segment-guest-elf` and `aggregator-guest-elf` CI artifacts.
# Needs SP1 v6.10.0 and protoc.
set -euo pipefail

cd "$(dirname "$0")/../.."
pins="$(pwd -P)/sp1/pins/tinyllama.proofs.sha256"
host=sp1/target/release/sp1-host

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

[[ -n "${PROOFS:-}" && -n "${ELF:-}" && -n "${AGGREGATOR_ELF:-}" ]] || fail "set PROOFS, ELF and AGGREGATOR_ELF"
[[ -f "$pins" ]] || fail "no $pins"
proofs="$(cd "$PROOFS" && pwd -P)"
[[ "$(find "$proofs" -mindepth 1 ! -type d | wc -l | tr -d ' ')" == 24 ]] ||
    fail "$proofs must hold exactly the 24 pinned proof files (make proofs-download)"
(cd "$proofs" && shasum -a 256 --strict --quiet -c "$pins")
echo "OK: the 24 proof files in $proofs match sp1/pins/tinyllama.proofs.sha256"

(cd sp1 && cargo build --release --locked -p sp1-host --bin sp1-host)

"$host" aggregate --segment-elf "$ELF" --aggregator-elf "$AGGREGATOR_ELF" --proofs "$proofs/segments" \
    --tags all --execute-only
