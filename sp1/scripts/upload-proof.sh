#!/usr/bin/env bash
# sp1/scripts/upload-proof.sh <file> <dir>: uploads one file to the Hugging Face dataset named by
# PROOFS_DATASET, as <dir>/<file's name>, in one commit (`hf upload`). gpu-prove.sh runs it through
# `sp1-host prove --then 'upload-proof.sh "$1" segments'`, so each proof is persisted before the next
# tag starts (docs/EVM-GAS-MODEL.md §9).
#
# The token comes only from the file named by HF_TOKEN_PATH, which huggingface_hub reads, mode 0600.
# HF_TOKEN itself must not be set: it would reach every process this one's parent starts, the builds,
# sp1-host and sp1-gpu-server among them, which inherit their parent's environment.
set -euo pipefail

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

(($# == 2)) || fail "usage: $0 <file> <dir>"
file=$1
dir=${2%/}
[[ -n "${PROOFS_DATASET:-}" ]] || fail "PROOFS_DATASET is unset"
[[ -z "${HF_TOKEN:-}" ]] || fail "HF_TOKEN is set; put the token in a 0600 file and name it with HF_TOKEN_PATH"
[[ -n "${HF_TOKEN_PATH:-}" && -f "$HF_TOKEN_PATH" && ! -L "$HF_TOKEN_PATH" ]] ||
    fail "HF_TOKEN_PATH must name the token file"
mode=$(stat -c %a "$HF_TOKEN_PATH" 2>/dev/null || stat -f %Lp "$HF_TOKEN_PATH")
[[ "$mode" == 600 ]] || fail "$HF_TOKEN_PATH has mode $mode, not 600"
[[ -f "$file" && ! -L "$file" ]] || fail "$file is not a regular file"
[[ "$dir" =~ ^[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)*$ ]] || fail "bad dataset directory $dir"
command -v hf >/dev/null || fail "no hf CLI (pip install huggingface_hub)"

name=$(basename "$file")
sha=$(sha256sum "$file" 2>/dev/null | cut -d' ' -f1 || shasum -a 256 "$file" | cut -d' ' -f1)
# Three attempts: a transient Hub error must not lose a proof that took GPU time to make. Uploading
# the same bytes again to the same path changes nothing in the dataset.
for attempt in 1 2 3; do
    if hf upload "$PROOFS_DATASET" "$file" "$dir/$name" --repo-type dataset \
        --commit-message "Add $dir/$name (sha256 $sha)"; then
        echo "uploaded $file to $PROOFS_DATASET as $dir/$name (sha256 $sha)"
        exit 0
    fi
    echo "WARNING: hf upload of $dir/$name failed (attempt $attempt of 3)" >&2
    ((attempt == 3)) || sleep $((attempt * 15))
done
fail "could not upload $file to $PROOFS_DATASET as $dir/$name"
