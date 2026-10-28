#!/usr/bin/env bash
# Builds the SP1 guests reproducibly in the SP1 v6.10.0 Docker image, pinned by digest (a tag is
# mutable), prints the segment guest's ELF sha256 and vkey, and fails unless they equal the committed
# sp1/segment-guest.elf.sha256 and sp1/segment-guest.vkey. Both cover every guest-linked file
# (sash-evm-primitives, the vendored SASH tree and its patches, sp1/Cargo.lock), so a change to any
# of them is re-committed from this script's output on the x86_64 CI runner.
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

for guest in segment-guest stwo-bench; do
    (cd "sp1/$guest" && cargo prove build --docker --tag v6.10.0 --locked --workspace-directory ../..)
done
(cd "$elf_dir" && shasum -a 256 segment-guest stwo-bench)

elf_sha256="$(cd "$elf_dir" && shasum -a 256 segment-guest)"
vkey="$(cd sp1 && cargo run --release --locked --quiet -p sp1-host --bin sp1-host -- vkey --elf "$root/$elf_dir/segment-guest")"
echo "segment-guest.elf.sha256: $elf_sha256"
echo "segment-guest.vkey:"
echo "$vkey"

[[ "$elf_sha256" == "$(cat sp1/segment-guest.elf.sha256 2>/dev/null)" ]] ||
    fail "the Docker-built segment guest's sha256 differs from sp1/segment-guest.elf.sha256; re-commit it and sp1/segment-guest.vkey from the x86_64 build above"
[[ "$vkey" == "$(cat sp1/segment-guest.vkey 2>/dev/null)" ]] ||
    fail "the Docker-built segment guest's vkey differs from sp1/segment-guest.vkey; re-commit it and sp1/segment-guest.elf.sha256 from the x86_64 build above"
echo "OK: the Docker-built segment guest matches sp1/segment-guest.elf.sha256 and sp1/segment-guest.vkey"
