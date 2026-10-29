#!/usr/bin/env bash
# make sp1-fixture-check: the M2 byte-identity gate (docs/ROADMAP.md). Runs the Docker-built segment
# guest on all 23 TinyLlama segments in the SP1 executor, renders TinyLlamaFixture.sol from the public
# values it commits (`sp1-host fixture-from-bundles`), and compares the result byte for byte with
# contracts/src/sash/TinyLlamaFixture.sol. The temporary directory is removed either way.
#
# BUNDLES: the 69 bundle files (`make bundles-download`, checked by `make bundles-check` first). ELF:
# the segment guest from
# `make sp1-docker`, or the `segment-guest-elf` CI artifact; it must match sp1/segment-guest.elf.sha256
# and sp1/segment-guest.vkey. A difference is a stop-and-ask: never edit the committed fixture, the Rust
# constants or SPEC.md §1.4 to match, and never run `make vectors` for it (AGENTS.md §2.4).
#
# The comparison does not cover modelId, EOS_TOKEN_ID or the prompt/response text: fixture-from-bundles
# fills them from tinyllama_fixture.rs, since they come from enrollment, not from the proofs.
#
# Not in `make check`; the sp1-bundles CI job runs it on pushes to main and the weekly schedule. Needs
# SP1 v6.10.0 and protoc.
set -euo pipefail

cd "$(dirname "$0")/../.."
fixture=contracts/src/sash/TinyLlamaFixture.sol
host=sp1/target/release/sp1-host

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

[[ -n "${BUNDLES:-}" && -n "${ELF:-}" ]] || fail "set BUNDLES and ELF"
bundles="$(cd "$BUNDLES" && pwd -P)"
elf="$(cd "$(dirname "$ELF")" && pwd)/$(basename "$ELF")"

(cd sp1 && cargo build --release --locked -p sp1-host --bin sp1-host)

# The ELF must be the committed program: #43's Docker build.
elf_sha="$(shasum -a 256 <"$elf" | cut -d' ' -f1)"
[[ "$elf_sha" == "$(cut -d' ' -f1 sp1/segment-guest.elf.sha256)" ]] ||
    fail "$elf has sha256 $elf_sha, not the one in sp1/segment-guest.elf.sha256; pass #43's Docker-built ELF"
vkey="$("$host" vkey --elf "$elf")"
[[ "$vkey" == "$(cat sp1/segment-guest.vkey)" ]] || fail "$elf's vkey differs from sp1/segment-guest.vkey: $vkey"
echo "OK: ELF sha256 $elf_sha matches sp1/segment-guest.elf.sha256"
echo "OK: its vkey matches sp1/segment-guest.vkey:"
echo "$vkey"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

tags=()
for l in $(seq 0 21); do tags+=("B0L$l"); done
tags+=(head)

for tag in "${tags[@]}"; do
    rc=0
    "$host" execute --elf "$elf" --bundle "$bundles" --tag "$tag" --public-values "$tmp/$tag.public-values" \
        >"$tmp/$tag.execute.txt" 2>&1 || rc=$?
    if [[ $rc != 0 ]]; then
        cat "$tmp/$tag.execute.txt"
        fail "segment $tag exits $rc, not 0. Stop and ask: never loosen a rule or edit a bundle"
    fi
    echo "OK: $tag exits 0"
done

"$host" fixture-from-bundles --public-values "$tmp" --bundles "$bundles" --prompt-len 25 \
    --out "$tmp/TinyLlamaFixture.sol" ||
    fail "fixture-from-bundles rejects the public values. Stop and ask"

if ! cmp "$tmp/TinyLlamaFixture.sol" "$fixture"; then
    diff -u "$fixture" "$tmp/TinyLlamaFixture.sol" || true
    fail "the fixture regenerated from the bundles differs from $fixture. Stop and ask: never edit the committed fixture to match"
fi
echo "regenerated sha256: $(shasum -a 256 <"$tmp/TinyLlamaFixture.sol" | cut -d' ' -f1)"
echo "committed sha256:   $(shasum -a 256 <"$fixture" | cut -d' ' -f1)"
echo "OK: the fixture regenerated from the 23 bundles is byte-identical to $fixture"
