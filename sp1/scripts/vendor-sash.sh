#!/usr/bin/env bash
# Builds two gitignored trees from SASH's source at the pinned commit, GitHub's tarball pinned by
# sha256 (SPEC.md §1.1):
# - sp1/vendor/sash: the tarball with sp1/patches/*.patch applied in order, the build the segment
#   guest and the segment check verify with;
# - sp1/vendor/sash-oracle: the tarball with only the patches that change behaviour (`behaviour`
#   below), the build sp1/sash-oracle runs as the differential tests' reference. The other patches
#   extract the verifier and must preserve its verdict, which those tests check.
# Each tree is always the tarball plus its patches; a hand edit under either is gone after the next run.
set -euo pipefail

cd "$(dirname "$0")/.."

commit=eb95bfa887299d1637550228d42086c27cbb2b3b
url="https://codeload.github.com/sg-ai-safety-hub/zk-inference-prover/tar.gz/${commit}"
sha256=98f3fbdcb2359898804ce06752db29afd046452704dcc5722c6bf18801827abe
tarball="vendor/zk-inference-prover-${commit}.tar.gz"
tree=vendor/sash

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

mkdir -p vendor
if [[ ! -f "$tarball" ]]; then
    curl -sSfL --retry 3 -o "$tarball.tmp" "$url" || fail "could not download $url"
    mv "$tarball.tmp" "$tarball"
fi
if [[ "$(sha256_of "$tarball")" != "$sha256" ]]; then
    rm -f "$tarball"
    fail "the downloaded tarball does not have the pinned sha256 $sha256 (removed it)"
fi

# The patches that change behaviour, on both the prover and the verifier side. They are written
# against the whole series, so on the bare tarball they may apply at a line offset, never with fuzz.
behaviour=(patches/0006-bind-token-vectors-into-transcript.patch)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# build_tree DEST OFFSETS PATCH...: the tarball plus PATCH... in order, into DEST. OFFSETS=no refuses a
# patch that applies at a line offset.
build_tree() {
    local dest="$1" offsets="$2"
    shift 2
    local src="$work/$(basename "$dest")"
    mkdir -p "$src"
    tar -xzf "$tarball" -C "$src" --strip-components=1
    [[ -d "$src/prover" ]] || fail "unexpected tarball layout"
    local p
    for p in "$@"; do
        # patch(1), not `git apply`: inside a git work tree `git apply` can skip a patch's paths and exit 0.
        patch -d "$src" -p1 -F0 --forward --batch --quiet <"$p" || fail "$p does not apply cleanly to SASH ${commit}"
    done
    # -F0 refuses fuzz but not an offset; patch(1) then leaves a backup. The tarball has no .orig or .rej.
    [[ -z "$(find "$src" -name '*.rej' -print -quit)" ]] || fail "a patch left a .rej in $dest"
    if [[ "$offsets" == no ]]; then
        local stray
        stray="$(find "$src" -name '*.orig' -print -quit)"
        [[ -z "$stray" ]] || fail "a patch applied at an offset or with fuzz (${stray#"$src"/})"
    else
        find "$src" -name '*.orig' -delete
    fi
    if [[ -d "$dest" ]] && diff -rq "$src" "$dest" >/dev/null 2>&1; then
        echo "OK: $dest is SASH ${commit} plus $# patch(es), unchanged"
        return
    fi
    rm -rf "$dest"
    mv "$src" "$dest"
    echo "OK: $dest rebuilt from SASH ${commit} plus $# patch(es)"
}

shopt -s nullglob
patches=(patches/*.patch)
for p in "${behaviour[@]}"; do
    [[ -f "$p" ]] || fail "$p (a behaviour patch) is missing"
done
build_tree "$tree" no ${patches[@]+"${patches[@]}"}
build_tree vendor/sash-oracle yes "${behaviour[@]}"
