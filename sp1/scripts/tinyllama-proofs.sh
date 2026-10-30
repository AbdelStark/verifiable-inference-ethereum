#!/usr/bin/env bash
# make proofs-download and the sp1-bundles CI job: downloads the 24 TinyLlama proof files pinned by
# sp1/pins/tinyllama.proofs.sha256 into DIR (default runs/tinyllama-proofs, gitignored), anonymously,
# from the public Hugging Face dataset below at its pinned commit, and fails unless DIR then holds
# exactly those 24 files with their pinned sha256. They are what `sp1/scripts/gpu-prove.sh all` uploaded
# (docs/EVM-GAS-MODEL.md §9):
#   segments/<tag>.proof.bin  the 23 segments' compressed proofs, B0L0..B0L21 and head (bincode
#                             SP1ProofWithPublicValues), the segment guest's under sp1/segment-guest.vkey;
#   plonk/tinyllama.plonk.bin the aggregator's PLONK proof over them, which sp1-proofs-check.sh
#                             (sp1-host plonk-fixture) requires to render to
#                             contracts/fixtures/tinyllama.plonk.json byte for byte.
# A file already in DIR with its pinned sha256 is kept, so a directory restored from a cache is
# checked, not downloaded again; any other file in DIR is an error. sp1/scripts/sp1-proofs-check.sh then
# verifies the 23 proofs (`sp1-host aggregate --proofs DIR/segments --tags all --execute-only`) and the
# PLONK proof against the fixture.
#
# PINS=write (after a gpu-prove.sh run, with REVISION set to the dataset commit that holds all 24):
# downloads the 24 files at REVISION into an empty DIR and writes sp1/pins/tinyllama.proofs.sha256 from
# them, in sha256sum format, the segments in segment order, then the PLONK proof. Never by hand.
#
# The proofs come from a public model, prompt and recipe (SPEC.md §1.4), so nothing in them is secret.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd -P)"

DATASET=0xchqrles/sash-tinyllama-sp1-proofs
# The dataset commit holding the 23 segment proofs and the PLONK proof.
REVISION=
pins="$root/sp1/pins/tinyllama.proofs.sha256"

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

(($# <= 1)) || fail "usage: [PINS=write] $0 [DIR]"
[[ "${PINS:-}" == "" || "${PINS:-}" == write ]] || fail "PINS must be unset or write"
[[ "$REVISION" =~ ^[0-9a-f]{40}$ ]] || fail "REVISION is not pinned to a dataset commit"
dir="${1:-$root/runs/tinyllama-proofs}"
mkdir -p "$dir"
cd "$dir"

sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }

# The 24 dataset paths, in pin order.
expected=()
for l in $(seq 0 21); do expected+=("segments/B0L$l.proof.bin"); done
expected+=(segments/head.proof.bin plonk/tinyllama.plonk.bin)

download() {
    mkdir -p "$(dirname "$1")"
    rm -f "$1" "$1.part"
    curl -fsSL --retry 5 --retry-all-errors -o "$1.part" \
        "https://huggingface.co/datasets/$DATASET/resolve/$REVISION/$1"
}

if [[ "${PINS:-}" == write ]]; then
    [[ -z "$(find . -mindepth 1 -print -quit)" ]] || fail "PINS=write needs an empty DIR, and $dir is not"
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    for name in "${expected[@]}"; do
        download "$name"
        mv "$name.part" "$name"
        echo "$(sha256 "$name")  $name" >>"$tmp"
        echo "downloaded $name"
    done
    mv "$tmp" "$pins"
    echo "wrote $pins from $DATASET at $REVISION"
fi

[[ -f "$pins" ]] || fail "no $pins"
names=()
while read -r hash name; do
    [[ "$hash" == \#* || -z "$hash" ]] && continue
    names+=("$name")
    if [[ -f "$name" && "$(sha256 "$name")" == "$hash" ]]; then
        continue
    fi
    download "$name"
    got="$(sha256 "$name.part")"
    [[ "$got" == "$hash" ]] || { rm -f "$name.part"; fail "$name downloaded with sha256 $got, not its pin $hash"; }
    mv "$name.part" "$name"
    echo "downloaded $name"
done <"$pins"

[[ "${names[*]}" == "${expected[*]}" ]] || fail "$pins must pin the 24 files ${expected[*]}, in that order"
[[ "$(find . -mindepth 1 ! -type d | wc -l | tr -d ' ')" == 24 &&
    "$(find . -mindepth 1 -type d | sort | xargs)" == "./plonk ./segments" ]] ||
    fail "$dir must hold exactly the 24 pinned proof files, under segments/ and plonk/"
shasum -a 256 --strict --quiet -c "$pins"
echo "OK: the 24 proof files in $dir match sp1/pins/tinyllama.proofs.sha256 ($DATASET at $REVISION)"
