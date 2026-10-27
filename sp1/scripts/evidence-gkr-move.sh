#!/usr/bin/env bash
# One-time evidence for issue #40 (removed before merge): the GKR-verifier move changes no behaviour.
# Compares a tree without patch 0002 (A: tarball + 0001) with the vendored tree (B: all patches).
set -euo pipefail
cd "$(dirname "$0")/.."
commit=eb95bfa887299d1637550228d42086c27cbb2b3b
work="$(mktemp -d)"
tar -xzf "vendor/zk-inference-prover-${commit}.tar.gz" -C "$work"
A="$work/zk-inference-prover-${commit}"
patch -d "$A" -p1 -F0 --forward --batch --quiet < patches/0001-gate-cuda-build-on-prover.patch
B="$PWD/vendor/sash"

for side in A B; do
    tree="${!side}"
    echo "== $side: stwo lookups tests with the prover feature"
    (cd "$tree/nitroo" && CARGO_TARGET_DIR="$PWD/../../../target/ev-$side" cargo test --release --locked -p stwo --features prover --lib -- lookups --skip cuda 2>&1 | tee "$work/tests-$side.log" | grep -E "^test |test result")
    grep -E "^test .* \.\.\. ok$" "$work/tests-$side.log" | sed -E 's/^test (.*) \.\.\. ok$/\1/' | sort >"$work/names-$side.txt"
done
echo "== test names, A vs B (utils' tests move from prover::lookups::utils to core::lookups::utils)"
diff "$work/names-A.txt" "$work/names-B.txt" || true
echo "== same names once the utils path is normalised"
diff <(sed 's/^prover::lookups::utils::/core::lookups::utils::/' "$work/names-A.txt" | sort) "$work/names-B.txt" && echo "identical"

echo "== oracle from A and from B on every corpus bundle"
cp -R sash-oracle "$work/oracle-A"
sed -i "s|path = \"../vendor/sash/prover\"|path = \"$A/prover\"|" "$work/oracle-A/Cargo.toml"
(cd "$work/oracle-A" && CARGO_TARGET_DIR="$PWD/../../target/sash" cargo build --release --locked 2>&1 | tail -1)
cp target/sash/release/sash-oracle "$work/oracle-A.bin" 2>/dev/null || cp "$work/oracle-A/../../target/sash/release/sash-oracle" "$work/oracle-A.bin"
(cd sash-oracle && CARGO_TARGET_DIR="$PWD/../target/sash" cargo build --release --locked 2>&1 | tail -1)
for tag in $(ls testdata/synthetic/*.fixed.json | xargs -n1 basename | sed 's/.fixed.json//'); do
    a="$("$work/oracle-A.bin" testdata/synthetic "$tag")"
    b="$(target/sash/release/sash-oracle testdata/synthetic "$tag")"
    if [[ "$a" == "$b" ]]; then echo "$tag: identical, accepts $(jq -r .accepts <<<"$b")"; else echo "$tag: DIFFERS"; exit 1; fi
done
