#!/usr/bin/env bash
# Prints the sha256 that keys the sash CI job's cache of the synthetic corpus and the built
# sash-oracle: one hash over the path and content of every file they are built from, checked against
# or run with. Any change to one of them misses the cache, and the sash job then runs in full
# (sash-synthetic.sh). The SASH tarball enters through its pinned commit and sha256 in
# vendor-sash.sh, the CUDA packages and the build environment through the sash job in ci.yml.
# Drift the repository cannot see (runner image, NVIDIA's apt repository) is not in the key: pushes
# to main and the weekly scheduled run never restore the cache.
set -euo pipefail

cd "$(dirname "$0")/../.."

inputs=(
    .github/workflows/ci.yml
    rust-toolchain.toml
    sp1/Cargo.lock
    sp1/patches
    sp1/sash-oracle
    sp1/sash-segment-check
    sp1/scripts/corpus-key.sh
    sp1/scripts/sash-corpus-check.sh
    sp1/scripts/sash-synthetic.sh
    sp1/scripts/vendor-sash.sh
    sp1/testdata/synthetic.oracle.json
    sp1/testdata/synthetic.sha256
)

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}

# Tracked files and new ones not yet added (never ignored ones such as target/ or vendor/), in byte
# order, so the key depends on the files alone.
files=()
for input in "${inputs[@]}"; do
    n=${#files[@]}
    while IFS= read -r -d '' f; do
        [[ -f "$f" ]] && files+=("$f")
    done < <(git ls-files -z --cached --others --exclude-standard -- "$input" | LC_ALL=C sort -zu)
    ((${#files[@]} > n)) || fail "no file under $input; update the inputs in $0"
done

sha256 "${files[@]}" | sha256 | cut -d' ' -f1
