#!/usr/bin/env bash
# Checks the synthetic bundle corpus in sp1/testdata/synthetic (gitignored) with upstream's oracle
# (sp1/sash-oracle, already built: SASH_ORACLE, default target/sash/release/sash-oracle): each
# bundle's accepts(), unbound edges and pinned proof.json digest; then the corpus files and the
# oracle's output against the committed records; then runs the segment check's bundle and
# differential tests (sp1/sash-segment-check, SPEC.md §1.1) on the corpus against the oracle.
# sash-synthetic.sh runs it on the corpus it has just generated; the sash CI job runs it alone on a
# corpus cache hit (sp1/scripts/corpus-key.sh), where nothing upstream is rebuilt. Linux only: the
# oracle links the CUDA runtime (no GPU needed). Needs sp1/vendor/sash (scripts/vendor-sash.sh).
#
# Records (committed): sp1/testdata/synthetic.sha256 and sp1/testdata/synthetic.oracle.json. A run
# fails unless the corpus and the oracle's output equal them. Every run also writes the fresh ones
# next to them (testdata/*.fresh, gitignored, uploaded by CI) so an intended change can be reviewed
# and committed from any host. SASH_SYNTHETIC_BOOTSTRAP=1 overwrites the records; so does a run with
# neither record yet, which then fails so that they get reviewed and committed. Either way the
# segment check's tests still run, on the records just written.
set -euo pipefail

cd "$(dirname "$0")/.."

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

for tool in jq sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || fail "'$tool' not found"
done
oracle="${SASH_ORACLE:-$PWD/target/sash/release/sash-oracle}"
corpus=testdata/synthetic
[[ -x "$oracle" ]] || fail "$oracle is not an executable sash-oracle; build it (sp1/scripts/sash-synthetic.sh)"
[[ -d "$corpus" ]] || fail "$corpus is missing; generate it (sp1/scripts/sash-synthetic.sh)"
[[ -d vendor/sash ]] || fail "vendor/sash is missing; run sp1/scripts/vendor-sash.sh first"

# tag | expected unbound edges | proof.json sha256 prefix (empty: none pinned upstream)
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
missing=0
[[ -f "${records[0]}" || -f "${records[1]}" ]] || missing=1
if [[ "${SASH_SYNTHETIC_BOOTSTRAP:-}" == 1 || "$missing" == 1 ]]; then
    cp testdata/synthetic.sha256.fresh "${records[0]}"
    cp testdata/synthetic.oracle.json.fresh "${records[1]}"
    echo "Wrote ${records[*]} from this run"
else
    for record in "${records[@]}"; do
        [[ -f "$record" ]] || fail "$record is missing while the other record exists; restore it from git"
        diff "$record" "$record.fresh" || fail "$record differs from this run ($record.fresh)"
    done
    echo "OK: corpus and oracle output match ${records[*]}"
fi

# The sp1/ workspace, on the repository's toolchain and in its own target directory.
echo "== segment check: bundle and differential tests on the corpus"
SASH_SYNTHETIC_CORPUS="$PWD/$corpus" SASH_ORACLE="$oracle" CARGO_TARGET_DIR="$PWD/target" \
    cargo test --locked -p sash-segment-check -- --ignored

[[ "$missing" == 0 ]] || fail "the records were missing; review and commit the ones this run wrote"
