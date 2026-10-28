#!/usr/bin/env bash
# Builds upstream SASH's prover (default features: SIMD backend, prover on) from the vendored tree,
# runs upstream's test suite (its pinned synthetic digests included), and generates the synthetic bundle corpus with the upstream
# oracle's verdict for each bundle. Then runs the segment check's bundle and differential tests
# (sp1/sash-segment-check, SPEC.md §1.1) on that corpus against the oracle. Linux only: the prover
# links the CUDA runtime (no GPU needed).
#
# Records (committed): sp1/testdata/synthetic.sha256 and sp1/testdata/synthetic.oracle.json.
# The bundles themselves stay in the gitignored sp1/testdata/synthetic/. A run fails unless the fresh
# corpus and oracle output equal the records. Every run also writes the fresh ones next to them
# (testdata/*.fresh, gitignored, uploaded by CI) so an intended change can be reviewed and committed
# from any host. SASH_SYNTHETIC_BOOTSTRAP=1 overwrites the records; so does a run with neither record
# yet, which then fails so that they get reviewed and committed.
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

# tag | expected unbound edges | proof.json sha256 prefix (empty: none pinned upstream)
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

expect() {
    local tag="$1" unbound="$2" digest="$3"
    local out accepts got_unbound sha
    out="$("$oracle" "$corpus" "$tag")"
    accepts="$(jq -r .accepts <<<"$out")"
    got_unbound="$(jq -c '.binding.unbound' <<<"$out")"
    sha="$(sha256sum "$corpus/$tag.proof.json" | cut -d' ' -f1)"
    echo "$tag: accepts $accepts, unbound $got_unbound, proof.json sha256 $sha"
    [[ "$accepts" == true ]] || fail "$tag: upstream accepts() is $accepts, expected true"
    [[ "$got_unbound" == "$unbound" ]] || fail "$tag: unbound $got_unbound, expected $unbound"
    [[ -z "$digest" || "$sha" == "$digest"* ]] || fail "$tag: proof.json sha256 $sha, pinned prefix $digest"
    jq -c --arg tag "$tag" '{($tag): .}' <<<"$out" >>"$corpus/.oracle.jsonl"
}
rm -f "$corpus/.oracle.jsonl"
expect sound_aligned '[]' db6d17a700c25457
expect aligned_gkr5 '[]' ''
expect sound_gkrfive '[3,5]' d4d90d4fbdedec94
expect skip_residual_stream '[1,2,3,5,6,7,8,9,10,11,13]' ''
expect skip_layer_boundary '[13]' ''
expect skip_kv_boundary '[14]' ''
expect skip_embedding '[15]' ''
expect head '[16]' e3f31c1d4aeae77e

jq -s 'add' "$corpus/.oracle.jsonl" >testdata/synthetic.oracle.json.fresh
rm -f "$corpus/.oracle.jsonl"
(cd "$corpus" && sha256sum -- * | sort -k2) >testdata/synthetic.sha256.fresh

records=(testdata/synthetic.sha256 testdata/synthetic.oracle.json)
if [[ "${SASH_SYNTHETIC_BOOTSTRAP:-}" == 1 || ( ! -f "${records[0]}" && ! -f "${records[1]}" ) ]]; then
    cp testdata/synthetic.sha256.fresh "${records[0]}"
    cp testdata/synthetic.oracle.json.fresh "${records[1]}"
    echo "Wrote ${records[*]} from this run"
    [[ "${SASH_SYNTHETIC_BOOTSTRAP:-}" == 1 ]] || fail "the records were missing; review and commit the ones this run wrote"
    exit 0
fi
for record in "${records[@]}"; do
    [[ -f "$record" ]] || fail "$record is missing while the other record exists; restore it from git"
    diff "$record" "$record.fresh" || fail "$record differs from this run ($record.fresh)"
done
echo "OK: corpus and oracle output match ${records[*]}"

# The sp1/ workspace, on the repository's toolchain and in its own target directory.
echo "== segment check: bundle and differential tests on the corpus"
SASH_SYNTHETIC_CORPUS="$PWD/$corpus" SASH_ORACLE="$oracle" CARGO_TARGET_DIR="$PWD/target" \
    cargo test --locked -p sash-segment-check -- --ignored
