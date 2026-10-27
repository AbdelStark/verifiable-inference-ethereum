#!/usr/bin/env bash
# Builds sp1/vendor/sash (gitignored): SASH's source at the pinned commit, from GitHub's tarball pinned
# by sha256, with sp1/patches/*.patch applied in order (SPEC.md §1.1, verifier build). The tree is
# always the tarball plus the patches; a hand edit under sp1/vendor/sash is gone after the next run.
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

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tar -xzf "$tarball" -C "$work"
src="$work/zk-inference-prover-${commit}"
[[ -d "$src" ]] || fail "unexpected tarball layout"

shopt -s nullglob
patches=(patches/*.patch)
n_patches=${#patches[@]}
for p in ${patches[@]+"${patches[@]}"}; do
    # patch(1), not `git apply`: inside a git work tree `git apply` can skip a patch's paths and exit 0.
    patch -d "$src" -p1 -F0 --forward --batch --quiet < "$p" || fail "$p does not apply cleanly to SASH ${commit}"
done
# -F0 refuses fuzz but not an offset; patch(1) then leaves a backup. The tarball has no .orig or .rej.
stray="$(find "$src" \( -name '*.orig' -o -name '*.rej' \) -print -quit)"
[[ -z "$stray" ]] || fail "a patch applied at an offset or with fuzz (${stray#"$src"/})"

if [[ -d "$tree" ]] && diff -rq "$src" "$tree" >/dev/null 2>&1; then
    echo "OK: $tree is SASH ${commit} plus ${n_patches} patch(es), unchanged"
    exit 0
fi
rm -rf "$tree"
mv "$src" "$tree"
echo "OK: $tree rebuilt from SASH ${commit} plus ${n_patches} patch(es)"
