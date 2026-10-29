#!/usr/bin/env bash
# make bundles-download and the sp1-bundles CI job: downloads the 69 TinyLlama proof bundle files
# pinned by sp1/pins/tinyllama.sha256 into DIR (default runs/tinyllama, gitignored), anonymously, from
# the public Hugging Face dataset below at its pinned commit, and fails unless DIR then holds exactly
# those 69 files with their pinned sha256. A file already in DIR with its pinned sha256 is kept, so a
# directory restored from a cache is checked, not downloaded again; any other file in DIR is an error.
#
# The bundles come from a public model, prompt and recipe (docs/EVM-GAS-MODEL.md), so nothing in them
# is secret. The dataset holds the 69 files and Hugging Face's .gitattributes, nothing else.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd -P)"

DATASET=0xchqrles/sash-tinyllama-bundles
REVISION=7284cdba1353dfd441fb4a244aabd0c9bfb9dcd5
pins="$root/sp1/pins/tinyllama.sha256"

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

(($# <= 1)) || fail "usage: $0 [DIR]"
dir="${1:-$root/runs/tinyllama}"
mkdir -p "$dir"
cd "$dir"

sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }

names=()
while read -r hash name; do
    [[ "$hash" == \#* || -z "$hash" ]] && continue
    names+=("$name")
    if [[ -f "$name" && "$(sha256 "$name")" == "$hash" ]]; then
        continue
    fi
    rm -f "$name" "$name.part"
    curl -fsSL --retry 5 --retry-all-errors -o "$name.part" \
        "https://huggingface.co/datasets/$DATASET/resolve/$REVISION/$name"
    got="$(sha256 "$name.part")"
    [[ "$got" == "$hash" ]] || { rm -f "$name.part"; fail "$name downloaded with sha256 $got, not its pin $hash"; }
    mv "$name.part" "$name"
    echo "downloaded $name"
done <"$pins"

[[ "${#names[@]}" == 69 ]] || fail "$pins must pin 69 files, not ${#names[@]}"
[[ "$(find . -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" == 69 ]] || fail "$dir must hold exactly the 69 pinned bundle files"
shasum -a 256 --strict --quiet -c "$pins"
echo "OK: the 69 bundle files in $dir match sp1/pins/tinyllama.sha256 ($DATASET at $REVISION)"
