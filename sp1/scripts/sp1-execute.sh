#!/usr/bin/env bash
# make sp1-execute: runs the Docker-built segment guest on the TinyLlama segments (all 23 by default)
# in the SP1 executor, one process per segment, checks each against native check_segment and its
# fixedHash pin, prints one row per segment and the totals (docs/EVM-GAS-MODEL.md §7), then runs the
# controls on modified copies of the bundles under OUT/controls.
#
# BUNDLES: the 69 bundle files (`make bundles-download`, checked by `make bundles-check` first). OUT:
# where the per-segment outputs go (outside the repository or gitignored); #44 and #47 read
# OUT/<tag>.public-values. ELF: the segment guest from `make sp1-docker`, the `segment-guest-elf` CI
# artifact, or copied from a host with the pinned image; a local non-Docker build depends on the
# installed toolchain, so its figures belong to no committed program. PINS=write writes
# sp1/pins/tinyllama.fixed.sha256 instead of checking it. SEGMENTS: a space-separated subset of the
# tags, in any order, which must include the controls' B0L0, B0L10 and head (the sp1-bundles CI job's
# PR path); PINS=write needs all 23.
#
# `make check` runs it on B0L0, B0L10 and head when make sp1-docker's ELF and the bundles are present;
# the sp1-bundles CI job runs it. Needs SP1 v6.10.0 and protoc.
set -euo pipefail

cd "$(dirname "$0")/../.."
root="$(pwd -P)"
pins=sp1/pins/tinyllama.fixed.sha256
host=sp1/target/release

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

[[ -n "${BUNDLES:-}" && -n "${OUT:-}" && -n "${ELF:-}" ]] || fail "set BUNDLES, OUT and ELF"
[[ "${PINS:-}" == "" || "${PINS:-}" == write ]] || fail "PINS must be unset or 'write'"
[[ -z "${SEGMENTS:-}" || "${PINS:-}" != write ]] || fail "PINS=write writes all 23 pins; unset SEGMENTS"

# Every variable SP1CoreOpts::default() reads (sp1-core-executor 6.10.0 src/opts.rs). Gas estimation
# treats each trace chunk as a shard, so the PGU depends on them; the recorded figures take none.
for var in MINIMAL_TRACE_CHUNK_THRESHOLD GAS_TRACE_CHUNK_THRESHOLD TRACE_CHUNK_SLOTS \
    GAS_TRACE_CHUNK_SLOTS MEMORY_LIMIT SHARD_SIZE ELEMENT_THRESHOLD HEIGHT_THRESHOLD FULL_SIZE_SHARDS; do
    [[ -z "${!var+set}" ]] || fail "$var is set; SP1CoreOpts reads it, and the recorded figures take SP1's defaults"
done

bundles="$(cd "$BUNDLES" && pwd -P)"
# OUT must be outside the repository or gitignored, and never the bundle directory or inside it. A
# rejected OUT that this run created is removed again.
[[ -d "$OUT" ]] && created= || created=1
mkdir -p "$OUT"
out="$(cd "$OUT" && pwd -P)"
reject() {
    [[ -z "$created" ]] || rmdir "$out"
    fail "$@"
}
case "$out/" in
"$bundles"/*) reject "OUT ($out) is the bundle directory or inside it" ;;
"$root"/*) git check-ignore -q "$out" || reject "OUT ($out) is inside the repository and not gitignored" ;;
esac
elf="$(cd "$(dirname "$ELF")" && pwd)/$(basename "$ELF")"

(
    cd sp1
    cargo build --release --locked -p sp1-host --bin sp1-host
    cargo build --release --locked -p sp1-host --features count-ops --bin native-check
    cargo build --release --locked -p sp1-host --features profiling --bin decode-share
)

# The ELF must be the committed program: #43's Docker build.
[[ "$(shasum -a 256 <"$elf" | cut -d' ' -f1)" == "$(cut -d' ' -f1 sp1/segment-guest.elf.sha256)" ]] ||
    fail "$elf does not have the sha256 in sp1/segment-guest.elf.sha256; pass #43's Docker-built ELF"
[[ "$("$host/sp1-host" vkey --elf "$elf")" == "$(cat sp1/segment-guest.vkey)" ]] ||
    fail "$elf's vkey differs from sp1/segment-guest.vkey"
echo "OK: $elf matches sp1/segment-guest.elf.sha256 and sp1/segment-guest.vkey"

all=()
for l in $(seq 0 21); do all+=("B0L$l"); done
all+=(head)
if [[ -z "${SEGMENTS:-}" ]]; then
    tags=("${all[@]}")
else
    read -ra tags <<<"$SEGMENTS"
    for tag in "${tags[@]}"; do
        [[ " ${all[*]} " == *" $tag "* ]] || fail "SEGMENTS: $tag is not a TinyLlama segment tag"
    done
    for tag in B0L0 B0L10 head; do
        [[ " ${tags[*]} " == *" $tag "* ]] || fail "SEGMENTS must include $tag: the controls run on it"
    done
fi

field() { sed -n "s/^$2: //p" "$1"; }

written=()
for tag in "${tags[@]}"; do
    pv="$out/$tag.public-values"
    rm -f "$pv"
    rc=0
    "$host/sp1-host" execute --elf "$elf" --bundle "$bundles" --tag "$tag" --public-values "$pv" \
        >"$out/$tag.execute.txt" || rc=$?
    case $rc in
    0) ;;
    1)
        cat "$out/$tag.execute.txt"
        fail "segment $tag exits 1 (rejected; see its stderr above). Stop and ask: never loosen a rule or edit a bundle"
        ;;
    3)
        cat "$out/$tag.execute.txt"
        fail "segment $tag hit sp1-host's CYCLE_LIMIT: not a rejection. Stop and ask"
        ;;
    *)
        cat "$out/$tag.execute.txt"
        fail "sp1-host execute failed on $tag (exit $rc)"
        ;;
    esac
    [[ "$(wc -c <"$pv" | tr -d ' ')" == 226 ]] || fail "$pv is not 226 bytes"
    "$host/native-check" --bundle "$bundles" --tag "$tag" --public-values "$pv" >"$out/$tag.native.txt" ||
        fail "the native check of $tag failed"
    "$host/decode-share" --bundle "$bundles" --tag "$tag" >"$out/$tag.decode.txt" ||
        fail "the decode measurement of $tag failed"
    hash="$(field "$out/$tag.native.txt" fixedHash)"
    if [[ "${PINS:-}" == write ]]; then
        written+=("$tag $hash")
    else
        pin="$(awk -v t="$tag" '$1 == t { print $2 }' "$pins")"
        [[ "$hash" == "$pin" ]] || fail "$tag: fixedHash $hash differs from its pin in $pins ($pin)"
    fi
    echo "OK: $tag exits 0 and its public values equal native check_segment's$([[ "${PINS:-}" == write ]] || echo ", fixedHash pinned")"
done

if [[ "${PINS:-}" == write ]]; then
    {
        echo "# fixedHash of each TinyLlama segment (SPEC.md §1.1), in segment order: sha256 of the"
        echo "# re-serialized parsed Fixed with tok_in and tok_out cleared. Not a file hash."
        echo "# Written by 'make sp1-execute PINS=write' (sp1/scripts/sp1-execute.sh); never by hand."
        printf '%s\n' "${written[@]}"
    } >"$pins"
    echo "OK: wrote $pins"
fi
[[ "$(grep -cv '^#' "$pins")" == "${#all[@]}" ]] || fail "$pins must hold one pin per segment"

# One row per segment, then the totals. Shares are of the segment's cycles.
for tag in "${tags[@]}"; do
    e="$out/$tag.execute.txt" n="$out/$tag.native.txt" d="$out/$tag.decode.txt"
    echo "$tag $(field "$e" 'exit code') $(field "$e" cycles) $(field "$e" PGU) $(field "$e" 'stdin bytes')" \
        "$(field "$e" 'peak RSS' | cut -d' ' -f1) $(field "$n" 'hash_node compressions')" \
        "$(field "$n" 'channel compressions') $(field "$n" 'BLAKE2s cycles' | cut -d' ' -f1)" \
        "$(field "$d" 'decode fixed+claims cycles') $(field "$d" 'decode proof cycles')"
done | awk '
    BEGIN {
        printf "%-6s %4s %14s %14s %11s %14s %10s %8s %7s %12s %12s %7s\n", "tag", "exit", "cycles", "PGU",
            "stdin B", "peak RSS B", "hash_node", "channel", "BLAKE2s", "dec F+C", "dec proof", "decode"
    }
    {
        printf "%-6s %4d %14d %14d %11d %14d %10d %8d %6.2f%% %12d %12d %6.2f%%\n", $1, $2, $3, $4, $5, $6, $7,
            $8, 100 * $9 / $3, $10, $11, 100 * ($10 + $11) / $3
        for (i = 3; i <= 11; i++) if (i != 6) s[i] += $i
        if ($6 > rss) rss = $6
    }
    END {
        printf "%-6s %4s %14d %14d %11d %14d %10d %8d %6.2f%% %12d %12d %6.2f%%\n", "total", "", s[3], s[4],
            s[5], rss, s[7], s[8], 100 * s[9] / s[3], s[10], s[11], 100 * (s[10] + s[11]) / s[3]
    }'
echo "(peak RSS: the largest segment's; BLAKE2s: counted compressions x stwo-bench's cycles per compression;"
echo " decode: stwo-bench's decode_fixed_claims and decode_proof regions)"

"$host/sp1-host" controls --elf "$elf" --bundles "$bundles" --out "$out" --pins "$pins"
