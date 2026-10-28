#!/usr/bin/env bash
# Builds upstream SASH's prover (default features: SIMD backend, prover on) from the vendored tree,
# runs upstream's test suite (its pinned synthetic digests included), builds upstream's oracle
# (sp1/sash-oracle) and generates the synthetic bundle corpus into the gitignored
# sp1/testdata/synthetic/. Then checks it with sash-corpus-check.sh: the oracle's verdict for each
# bundle, the committed records (sp1/testdata/synthetic.sha256 and synthetic.oracle.json; see there
# for the fresh records every run writes and SASH_SYNTHETIC_BOOTSTRAP), and the segment check's
# bundle and differential tests (sp1/sash-segment-check, SPEC.md §1.1) on that corpus against the
# oracle. Linux only: the prover links the CUDA runtime (no GPU needed).
set -euo pipefail

cd "$(dirname "$0")/.."

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

[[ "$(uname -s)" == Linux ]] || fail "SASH's prover links the CUDA runtime and builds only on Linux"
command -v nvcc >/dev/null 2>&1 || fail "nvcc not found: install the CUDA 12.9 toolkit (no GPU needed)"
[[ -e /usr/local/cuda/lib64/libcudart.so ]] || fail "/usr/local/cuda/lib64/libcudart.so not found"
for tool in jq sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || fail "'$tool' not found"
done

scripts/vendor-sash.sh
export CARGO_TARGET_DIR="$PWD/target/sash"

echo "== build zk-prover (SIMD) and run upstream's whole test suite, as its CI does (pinned synthetic digests included)"
(cd vendor/sash && cargo build --release --locked && cargo test --release --locked)
echo "== build sash-oracle"
(cd sash-oracle && cargo build --release --locked)
prover="$CARGO_TARGET_DIR/release/zk-prover"
oracle="$CARGO_TARGET_DIR/release/sash-oracle"

corpus=testdata/synthetic
rm -rf "$corpus"
mkdir -p "$corpus"

syn=(compose --round away --variants intsplit6 --table-bits 16 --s 32 --ctx 128 --hd 32)
aligned=(--head-groups 2,2 --nin 128 --nout 128 --residual-stream-a 32 --kmax 16 --synth-residual-stream --diagonal-block)
gkr5=(--gkr-key-tiles --gkr-dequant --gkr-residual-stream --gkr-freivalds --gkr-attnout)
narrow=(--head-groups 2,1 --nout 64 --residual-stream-a 32 --kmax 16)

compose() {
    local tag="$1"
    shift
    echo "== compose $tag"
    RAYON_NUM_THREADS=4 "$prover" "${syn[@]}" "$@" --ser-dir "$corpus" --ser-tag "$tag" >"$corpus/$tag.compose.out"
}
compose sound_aligned "${aligned[@]}" --layer 1 --block 0
compose aligned_gkr5 "${aligned[@]}" "${gkr5[@]}" --layer 1 --block 0
compose sound_gkrfive "${narrow[@]}" --synth-residual-stream --diagonal-block "${gkr5[@]}" --layer 1 --block 0
compose skip_residual_stream "${aligned[@]}" --unsound-skip-residual-stream --layer 1 --block 0
compose skip_layer_boundary "${aligned[@]}" --unsound-skip-layer-boundary --layer 1 --block 0
compose skip_kv_boundary "${aligned[@]}" --unsound-skip-kv-boundary --layer 1 --block 0
compose skip_embedding "${aligned[@]}" --unsound-skip-embedding --layer 0 --block 0
compose head "${narrow[@]}" --synth-head --synth-vocab 256 --unsound-skip-argmax --layer 22 --block 0
rm -f "$corpus"/*.compose.out

SASH_ORACLE="$oracle" scripts/sash-corpus-check.sh
