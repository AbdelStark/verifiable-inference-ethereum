#!/usr/bin/env bash
# Builds the SP1 guests reproducibly in the SP1 v6.10.0 Docker image, pinned by digest (a tag is
# mutable), prints the segment and aggregator guests' ELF sha256 and vkey, and fails unless they equal
# the committed sp1/<guest>.elf.sha256 and sp1/<guest>.vkey. The pins cover every guest-linked file
# (sash-evm-primitives, sash-aggregate, the vendored SASH tree and its patches, sp1/Cargo.lock, and
# for the aggregator its SEGMENT_VKEY), so a change to any of them is re-committed from this script's
# output on the x86_64 CI runner. The aggregator's vkey `bytes32` is SASH_AGGREGATOR_VKEY (SPEC.md
# §1.2). The control-only echo guest is not pinned (SPEC.md §2.1, B2).
#
# Needs sp1/vendor/sash (sp1/scripts/vendor-sash.sh): Docker mounts only the repository. sp1-build
# runs the image as linux/amd64, so an arm64 host builds under emulation, which hangs.
set -euo pipefail

cd "$(dirname "$0")/../.."
root=$PWD

export SP1_DOCKER_IMAGE=ghcr.io/succinctlabs/sp1@sha256:9363a70004271de441e863612c7aefbbc5c36f371f111cd8abb1672fa5ea99f1
elf_dir=sp1/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

[[ -d sp1/vendor/sash ]] || fail "sp1/vendor/sash is missing; run sp1/scripts/vendor-sash.sh first"

pinned=(segment-guest aggregator-guest)
for guest in "${pinned[@]}" stwo-bench; do
    (cd "sp1/$guest" && cargo prove build --docker --tag v6.10.0 --locked --workspace-directory ../..)
done
(cd "$elf_dir" && shasum -a 256 "${pinned[@]}" stwo-bench)

# Every pinned guest's values are printed before any check fails, so one CI run gives them all.
mismatch=
for guest in "${pinned[@]}"; do
    elf_sha256="$(cd "$elf_dir" && shasum -a 256 "$guest")"
    vkey="$(cd sp1 && cargo run --release --locked --quiet -p sp1-host --bin sp1-host -- vkey --elf "$root/$elf_dir/$guest")"
    echo "$guest.elf.sha256: $elf_sha256"
    echo "$guest.vkey:"
    echo "$vkey"
    if [[ "$elf_sha256" == "$(cat "sp1/$guest.elf.sha256" 2>/dev/null)" && "$vkey" == "$(cat "sp1/$guest.vkey" 2>/dev/null)" ]]; then
        echo "OK: the Docker-built $guest matches sp1/$guest.elf.sha256 and sp1/$guest.vkey"
    else
        echo "ERROR: the Docker-built $guest's sha256 or vkey differs from sp1/$guest.elf.sha256 and sp1/$guest.vkey; re-commit both from the x86_64 build above" >&2
        mismatch=1
    fi
done
[[ -z "$mismatch" ]] || exit 1
