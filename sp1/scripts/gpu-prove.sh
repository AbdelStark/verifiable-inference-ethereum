#!/usr/bin/env bash
# sp1/scripts/gpu-prove.sh smoke|all|wrap: the M3 GPU pipeline (docs/EVM-GAS-MODEL.md §9), run from a
# checkout on a fresh Linux x86_64 host with an NVIDIA GPU of compute capability 8.0 or more, at least
# 24 GB of VRAM and a driver for CUDA 12.8. Any provider; the Hugging Face Jobs launcher stays outside
# the repository (§9 gives its command line).
#
# Setup: apt, rustup (the repository's toolchain), sp1up v6.10.0, protoc, vendor-sash.sh, then
# `cargo build --release --locked -p sp1-host`, and sp1-gpu-server v6.10.0 fetched into ~/.sp1/bin
# against its pinned sha256, as sp1-cuda would on the first client build (the warm-up, so no timed
# step pays for it). SP1_PLONK_CIRCUIT_PATH points at the largest writable disk before any sp1-host
# starts: sp1-gpu-server inherits sp1-host's environment, downloads the PLONK circuit there (into
# $SP1_PLONK_CIRCUIT_PATH/v6.1.0/, sp1-prover 6.10.0 src/build.rs) and runs gnark natively, with no
# Docker (docs/EVM-GAS-MODEL.md §9.3).
#
# Inputs: the 69 TinyLlama bundle files (tinyllama-bundles.sh, anonymous; smoke and all only, since
# wrap proves no segment); the two Docker-built ELFs
# from the proofs dataset's elf/ at ELF_REVISION (or from ELF_DIR, holding segment-guest and
# aggregator-guest), checked against sp1/<guest>.elf.sha256; and the proofs already in the dataset's
# segments/, so that `prove` proves only the missing tags.
#
# Modes, each step an sp1-host process with its own CUDA client:
#   smoke  B0L10 proved and uploaded to segments/; the honest N = 1 compressed aggregation (T1);
#          binding controls B1 and B2 (SPEC.md §2.1), each capped at CONTROL_CAP, a timeout reported
#          as inconclusive, and not run if the honest N = 1 aggregation failed; the N = 2 compressed aggregation of B0L10 twice (T2); the N = 1 PLONK wrap,
#          whose programVKey, publicValues and proofBytes are printed for the Foundry check.
#   all    every missing segment proved and uploaded one by one; if all 23 are in place, the
#          compressed aggregation of the 23 (timing only), then their PLONK aggregation with
#          --fixture-out, its proof uploaded to plonk/ and the fixture printed with its sha256. A re-run
#          (the resume) proves only the tags missing from segments/, then aggregates.
#   wrap   the contingency wrap-only run: the PLONK circuit downloaded with curl and extracted into
#          $SP1_PLONK_CIRCUIT_PATH/v6.1.0/ with its empty .complete marker, then the N = 1 PLONK
#          aggregation of the persisted B0L10 proof.
# Each step starts once the previous step's sp1-gpu-server has exited and the GPU's memory.used is
# back to its idle level, then samples memory.used every second. Each PLONK step prints `df -h` before
# and after, and samples `du` of $SP1_PLONK_CIRCUIT_PATH, which holds sp1-prover's staging directory
# (the tarball, then the tree extracted beside it) and the installed circuit: its peak circuit disk and
# the tarball's download and extraction times. The bundles and proofs are deleted on EXIT, after the
# uploads have returned.
#
# Environment:
#   HF_TOKEN_PATH  (smoke, all) a 0600 file holding a token that can write to the dataset; read only by
#                  upload-proof.sh's `hf`. HF_TOKEN must be unset: builds, sp1-host and sp1-gpu-server
#                  inherit this script's environment.
#   GPU_PROVE_WORK the work directory, outside the checkout (default: gpu-prove/ on the largest
#                  writable disk).
#   ELF_DIR        the ELFs from a local directory instead of the dataset.
#   NO_GPU=1       stops after the setup and the inputs: checks a host before any GPU time.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd -P)"

# The public dataset of the ELFs and proofs, and its commit holding elf/: the Docker builds of CI run
# 41051189040 (commit 96f8d3b), which match sp1/<guest>.elf.sha256.
DATASET=0xchqrles/sash-tinyllama-sp1-proofs
ELF_REVISION=29d003c1a7f6e94c6bcdf9e9473955cbf41dc768
# sp1-sdk 6.10.0 downloads this asset on the first CUDA client build (sp1-cuda src/server.rs); its
# sha256 as downloaded on 2026-10-28 (docs/EVM-GAS-MODEL.md §8).
GPU_SERVER_URL=https://github.com/succinctlabs/sp1/releases/download/v6.10.0/sp1_gpu_server_v6.10.0_x86_64.tar.gz
GPU_SERVER_SHA256=20ee4b96fa6e5a2ce8a9da523ee42b75c698658ae1d4366e57b52bdd642f1bdb
SP1UP_URL=https://raw.githubusercontent.com/succinctlabs/sp1/v6.10.0/sp1up/sp1up
# The v6.1.0 PLONK circuit sp1-prover 6.10.0 downloads (src/build.rs); `wrap` fetches it with curl.
PLONK_CIRCUIT_URL=https://sp1-circuits.s3-us-east-2.amazonaws.com/v6.1.0-plonk.tar.gz
PLONK_CIRCUIT_VERSION=v6.1.0
HF_HUB_VERSION=2.4.0
CONTROL_CAP=6m
# A step starts once memory.used is within GPU_IDLE_SLACK MiB of the idle GPU's, waiting at most
# GPU_SETTLE_MAX seconds.
GPU_IDLE_SLACK=256
GPU_SETTLE_MAX=60

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

(($# == 1)) && [[ "$1" =~ ^(smoke|all|wrap)$ ]] || fail "usage: $0 smoke|all|wrap"
mode=$1
uploads=$([[ "$mode" == wrap ]] && echo no || echo yes)
[[ -z "${HF_TOKEN:-}" ]] || fail "HF_TOKEN is set; put the token in a 0600 file and name it with HF_TOKEN_PATH"
if [[ "$uploads" == yes && -z "${NO_GPU:-}" ]]; then
    [[ -n "${HF_TOKEN_PATH:-}" && -f "$HF_TOKEN_PATH" ]] || fail "HF_TOKEN_PATH must name the token file"
fi
[[ -n "$ELF_REVISION" || -n "${ELF_DIR:-}" ]] || fail "ELF_REVISION is not pinned; set ELF_DIR"
[[ "$(uname -sm)" == "Linux x86_64" ]] || fail "needs Linux x86_64 (sp1-gpu-server's only build)"

t0=$(date +%s)
phase() { echo "== [$(($(date +%s) - t0))s] $*"; }
sudo=$([[ $(id -u) == 0 ]] || echo sudo)

# The largest writable mount: the work directory and the PLONK circuit go there.
largest_disk() {
    local mounts target
    mapfile -t mounts < <(df -P -k | awk 'NR > 1 { print $4, $6 }' | sort -rn)
    for target in "${mounts[@]}"; do
        target=${target#* }
        if [[ -d "$target" && -w "$target" ]]; then
            echo "${target%/}"
            return
        fi
    done
    echo "$HOME"
}
work=${GPU_PROVE_WORK:-$(largest_disk)/gpu-prove}
mkdir -p "$work"
work="$(cd "$work" && pwd -P)"
[[ "$work/" != "$root/"* ]] || fail "the work directory $work is inside the checkout"
bundles=$work/bundles
proofs=$work/proofs
out=$work/out
results=$work/results
elf_dir=$work/elf
mkdir -p "$proofs" "$out" "$results" "$elf_dir"
export SP1_PLONK_CIRCUIT_PATH=$work/circuits/plonk
export CARGO_TARGET_DIR=$work/target
export PROOFS_DATASET=$DATASET

samplers=()
cleanup() {
    for pid in "${samplers[@]}"; do kill "$pid" 2>/dev/null || true; done
    pkill -x sp1-gpu-server 2>/dev/null || true
    # The bundles and the proofs never outlive the run; every upload has returned by now.
    rm -rf "$bundles" "$proofs" "$out" "$work/hf"
    phase "deleted the bundles and the proofs"
}
trap cleanup EXIT

phase "host ($mode)"
date -u +%Y-%m-%dT%H:%M:%SZ
uname -srm
git -C "$root" log -1 --format='commit %H %s' || true
nproc
free -g || true
df -h
echo "work: $work"
echo "SP1_PLONK_CIRCUIT_PATH: $SP1_PLONK_CIRCUIT_PATH"
if [[ -z "${NO_GPU:-}" ]]; then
    nvidia-smi -q | sed -n '1,/^GPU /p'
    gpu=$(nvidia-smi --query-gpu=name,compute_cap,memory.total,driver_version,power.limit --format=csv,noheader,nounits | head -1)
    echo "GPU: $gpu"
    cap=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader | head -1)
    vram=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits | head -1)
    cuda=$(nvidia-smi | sed -n 's/.*CUDA Version: *\([0-9.]*\).*/\1/p' | head -1)
    awk -v c="$cap" 'BEGIN { exit !(c >= 8.0) }' || fail "compute capability $cap < 8.0"
    ((vram >= 24 * 1024)) || fail "$vram MiB of VRAM < 24 GB"
    awk -v v="$cuda" 'BEGIN { split(v, a, "."); exit !(a[1] > 12 || (a[1] == 12 && a[2] >= 8)) }' ||
        fail "the driver supports CUDA $cuda, not 12.8"
fi

phase "apt"
export DEBIAN_FRONTEND=noninteractive
$sudo apt-get update -qq
# libprotobuf-dev: the well-known .proto files sp1-prover-types imports. perl: shasum, for
# tinyllama-bundles.sh.
$sudo apt-get install -y -qq --no-install-recommends build-essential curl ca-certificates git \
    patch pkg-config protobuf-compiler libprotobuf-dev perl python3 python3-venv time procps util-linux >/dev/null
protoc --version

phase "rustup"
export PATH=$HOME/.cargo/bin:$HOME/.sp1/bin:$PATH
command -v rustup >/dev/null || curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain none
(cd "$root" && rustup toolchain install && rustc --version)

phase "sp1up v6.10.0"
if [[ "$(cargo prove --version 2>/dev/null)" != *6.10.0* ]]; then
    curl -sSfL -o "$work/sp1up" "$SP1UP_URL"
    bash "$work/sp1up" --version v6.10.0 >"$results/sp1up.log" 2>&1 || { tail -40 "$results/sp1up.log"; exit 1; }
fi
cargo prove --version

phase "vendor SASH"
"$root/sp1/scripts/vendor-sash.sh"

phase "build sp1-host (release; builds the guests)"
(cd "$root/sp1" && cargo build --release --locked -p sp1-host --bin sp1-host)
host=$CARGO_TARGET_DIR/release/sp1-host

phase "huggingface_hub $HF_HUB_VERSION"
python3 -m venv "$work/venv"
"$work/venv/bin/pip" install -q "huggingface_hub==$HF_HUB_VERSION"
export PATH=$PATH:$work/venv/bin
hf version

phase "warm-up: sp1-gpu-server v6.10.0 into ~/.sp1/bin, as sp1-cuda fetches it"
curl -sSfL -o "$work/gpu-server.tar.gz" "$GPU_SERVER_URL"
echo "$GPU_SERVER_SHA256  $work/gpu-server.tar.gz" | sha256sum --strict -c
mkdir -p "$HOME/.sp1/bin"
tar -xzf "$work/gpu-server.tar.gz" -C "$HOME/.sp1/bin"
rm "$work/gpu-server.tar.gz"
echo "sp1-gpu-server sha256: $(sha256sum "$HOME/.sp1/bin/sp1-gpu-server" | cut -d' ' -f1)"
if [[ -z "${NO_GPU:-}" ]]; then
    version=$("$HOME/.sp1/bin/sp1-gpu-server" --version | xargs)
    echo "sp1-gpu-server --version: $version"
    # sp1-cuda replaces a server of another version through `systemctl --user`, which a container
    # lacks: fail here, before any GPU time.
    [[ "$version" == 6.10.0 ]] || fail "sp1-gpu-server is not 6.10.0"
fi

if [[ "$mode" != wrap ]]; then
    phase "bundles"
    "$root/sp1/scripts/tinyllama-bundles.sh" "$bundles"
fi

phase "ELFs"
for guest in segment-guest aggregator-guest; do
    if [[ -n "${ELF_DIR:-}" ]]; then
        install -m 644 "$ELF_DIR/$guest" "$elf_dir/$guest"
    else
        curl -fsSL --retry 5 --retry-all-errors -o "$elf_dir/$guest" \
            "https://huggingface.co/datasets/$DATASET/resolve/$ELF_REVISION/elf/$guest"
    fi
    (cd "$elf_dir" && sha256sum --strict -c "$root/sp1/$guest.elf.sha256")
done
echo "OK: both ELFs match sp1/<guest>.elf.sha256 (${ELF_DIR:-$DATASET at $ELF_REVISION})"
segment_elf=$elf_dir/segment-guest
aggregator_elf=$elf_dir/aggregator-guest

phase "proofs already in $DATASET's segments/"
"$work/venv/bin/hf" download "$DATASET" --repo-type dataset --include 'segments/*.proof.bin' \
    --local-dir "$work/hf" >/dev/null
find "$work/hf/segments" -maxdepth 1 -name '*.proof.bin' -type f -exec mv -n -t "$proofs" {} + 2>/dev/null || true
ls -l "$proofs"

if [[ -n "${NO_GPU:-}" ]]; then
    phase "NO_GPU: stopping before the GPU steps"
    exit 0
fi

gpu_used() { nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | head -1; }
gpu_idle=$(gpu_used)
echo "idle GPU memory.used: $gpu_idle MiB"

# Whether an sp1-gpu-server is running (a zombie holds no GPU memory, and pgrep counts it).
# shellcheck disable=SC2009
server_running() { ps -C sp1-gpu-server -o stat= | grep -qv '^Z'; }

# Waits until no sp1-gpu-server runs and memory.used is back to idle. A step's server dies when its
# client drops (sp1-cuda's kill_on_drop sends SIGKILL and does not wait), and the driver frees its
# memory after that, so without the wait a step's first samples can be the previous step's server.
gpu_settle() {
    local waited=0
    while server_running || (($(gpu_used) > gpu_idle + GPU_IDLE_SLACK)); do
        if ((waited >= GPU_SETTLE_MAX)); then
            echo "WARNING: after $waited s, the GPU is not idle: memory.used $(gpu_used) MiB"
            ps -C sp1-gpu-server -o pid=,stat=,etime= || true
            return 1
        fi
        sleep 1
        waited=$((waited + 1))
    done
    echo "GPU idle after $waited s: memory.used $(gpu_used) MiB"
}

# One sp1-host step: GPU memory sampled every second, its own log, the wall clock and the peak RSS of
# sp1-host and its exited children; prints the exit code. `cap` bounds the wall time (0: none).
step() {
    local name=$1 cap=$2
    shift 2
    phase "$name"
    local bound=""
    gpu_settle || bound=" (an upper bound: the GPU was not idle when the step started)"
    nvidia-smi --query-gpu=timestamp,memory.used,utilization.gpu --format=csv,noheader,nounits -l 1 \
        >"$results/$name.gpu.csv" &
    local sampler=$!
    samplers+=("$sampler")
    steps+=("$name")
    local rc=0
    if [[ "$cap" == 0 ]]; then
        RUST_LOG=info NO_COLOR=1 /usr/bin/time -v -o "$results/$name.time" "$host" "$@" 2>&1 |
            tee "$results/$name.log" || rc=$?
    else
        RUST_LOG=info NO_COLOR=1 /usr/bin/time -v -o "$results/$name.time" \
            timeout --kill-after=30s "$cap" "$host" "$@" 2>&1 | tee "$results/$name.log" || rc=$?
    fi
    kill "$sampler" 2>/dev/null || true
    # A killed sp1-host leaves its sp1-gpu-server running (kill_on_drop needs a drop).
    [[ "$rc" == 0 ]] || pkill -x sp1-gpu-server 2>/dev/null || true
    echo "$rc" >"$results/$name.rc"
    awk -F', ' -v b="$bound" '{ if ($2 > m) m = $2 }
        END { printf "peak GPU memory.used: %d MiB over %d samples%s\n", m, NR, b }' \
        "$results/$name.gpu.csv" | tee "$results/$name.gpu-peak"
    grep -E "Elapsed|Maximum resident" "$results/$name.time" || true
    echo "exit code: $rc"
    return "$rc"
}

# Every second during a PLONK step: the time, the kB under $SP1_PLONK_CIRCUIT_PATH and the bytes of
# the tarball sp1-prover downloads into its staging directory there (0 when absent). sp1-prover 6.10.0
# (src/build.rs) writes it to $SP1_PLONK_CIRCUIT_PATH/v6.1.0.incomplete.*/artifacts.tar.gz, extracts it
# beside itself, deletes it and renames the directory to v6.1.0. A df of that disk would count other
# tenants' writes to a shared volume.
disk_sampler() {
    local tarball
    while sleep 1; do
        tarball=$(stat -c %s "$SP1_PLONK_CIRCUIT_PATH"/*.incomplete.*/artifacts.tar.gz 2>/dev/null | head -1) || true
        echo "$(date +%s) $(du -sk "$SP1_PLONK_CIRCUIT_PATH" 2>/dev/null | cut -f1) ${tarball:-0}"
    done
}

# The peak circuit disk and, if the step downloaded the circuit, the tarball's size, download time (its
# first sample to the first at its final size) and extraction time (from then to its removal), at the
# samples' 1 s resolution.
disk_report() {
    awk '{ if ($2 > d) d = $2
           if ($3 > 0) { if (!t0) t0 = $1; last = $1; if ($3 > s) { s = $3; full = $1 } } }
         END {
           printf "peak circuit disk: %d MiB (du of SP1_PLONK_CIRCUIT_PATH, staging directory included; %d samples)\n", d / 1024, NR
           if (s) {
             printf "circuit tarball: %d bytes, downloaded in about %d s\n", s, full - t0
             printf "circuit extraction: about %d s, until the tarball was deleted\n", last + 1 - full
           } else print "circuit tarball: not seen (the circuit was already installed)"
         }' "$1"
}

plonk_step() {
    local name=$1
    shift
    mkdir -p "$SP1_PLONK_CIRCUIT_PATH"
    echo "--- df -h before $name"
    df -h
    disk_sampler >"$results/$name.disk" &
    local sampler=$!
    samplers+=("$sampler")
    local rc=0
    step "$name" 0 "$@" || rc=$?
    kill "$sampler" 2>/dev/null || true
    echo "--- df -h after $name"
    df -h
    du -sh "$SP1_PLONK_CIRCUIT_PATH"/* 2>/dev/null || true
    disk_report "$results/$name.disk" | tee "$results/$name.disk-peak"
    return "$rc"
}

upload_then="'$root/sp1/scripts/upload-proof.sh' \"\$1\" segments"
steps=()
failed=()
notes=()
case "$mode" in
smoke)
    step prove-B0L10 0 prove --elf "$segment_elf" --bundle "$bundles" --tags B0L10 --out-dir "$proofs" \
        --then "$upload_then" || fail "B0L10 did not prove: the other steps need its proof"
    agg=(--segment-elf "$segment_elf" --aggregator-elf "$aggregator_elf" --proofs "$proofs")
    baseline=0
    step aggregate-n1 0 aggregate "${agg[@]}" --tags B0L10 --mode compressed --out "$out/n1.compressed.bin" ||
        baseline=$?
    ((baseline == 0)) || failed+=(aggregate-n1)
    for kind in pv-byte vk; do
        # A control passes on any proving error, so without a verifying honest baseline its pass would
        # show nothing about the binding (SPEC.md §2.1).
        if ((baseline != 0)); then
            failed+=("control-$kind")
            notes+=("control $kind: NOT RUN (honest baseline failed)")
            echo "${notes[-1]}"
            continue
        fi
        rc=0
        step "control-$kind" "$CONTROL_CAP" aggregate-control --kind "$kind" "${agg[@]}" || rc=$?
        case "$rc" in
        0) outcome="PASS (no verifying proof)" ;;
        1) outcome="FAIL (a verifying proof)" ;;
        124 | 137) outcome="INCONCLUSIVE (over $CONTROL_CAP)" ;;
        *) outcome="ERROR (exit $rc)" ;;
        esac
        [[ "$rc" == 0 ]] || failed+=("control-$kind")
        echo "control $kind: $outcome" | tee "$results/control-$kind.outcome"
    done
    step aggregate-n2 0 aggregate "${agg[@]}" --tags B0L10,B0L10 --mode compressed --out "$out/n2.compressed.bin" ||
        failed+=(aggregate-n2)
    plonk_step aggregate-n1-plonk aggregate "${agg[@]}" --tags B0L10 --mode plonk --out "$out/n1.plonk.bin" ||
        failed+=(aggregate-n1-plonk)
    ;;
all)
    step prove-all 0 prove --elf "$segment_elf" --bundle "$bundles" --tags all --out-dir "$proofs" \
        --then "$upload_then" || fail "a segment did not prove; re-run to prove the missing tags"
    agg=(--segment-elf "$segment_elf" --aggregator-elf "$aggregator_elf" --proofs "$proofs" --tags all)
    # The deferred recursion over the 23 proofs and the compress stage alone, which the PLONK step's
    # log does not separate from shrink and wrap; its proof is not kept.
    step aggregate-all-compressed 0 aggregate "${agg[@]}" --mode compressed --out "$out/tinyllama.compressed.bin" ||
        failed+=(aggregate-all-compressed)
    plonk_step aggregate-all-plonk aggregate "${agg[@]}" --mode plonk --out "$out/tinyllama.plonk.bin" \
        --fixture-out "$out/tinyllama.plonk.json" || failed+=(aggregate-all-plonk)
    if [[ -f "$out/tinyllama.plonk.bin" ]]; then
        "$root/sp1/scripts/upload-proof.sh" "$out/tinyllama.plonk.bin" plonk
        # The fixture as --fixture-out wrote it, and its sha256 to check a copy taken from this log.
        echo "--- tinyllama.plonk.json"
        cat "$out/tinyllama.plonk.json"
        echo "tinyllama.plonk.json sha256: $(sha256sum "$out/tinyllama.plonk.json" | cut -d' ' -f1)"
    fi
    ;;
wrap)
    [[ -f "$proofs/B0L10.proof.bin" ]] || fail "no B0L10.proof.bin in $DATASET's segments/"
    phase "PLONK circuit $PLONK_CIRCUIT_VERSION pre-extracted (curl)"
    dir=$SP1_PLONK_CIRCUIT_PATH/$PLONK_CIRCUIT_VERSION
    [[ ! -e "$dir" ]] || fail "$dir exists"
    mkdir -p "$dir"
    df -h
    # The tarball beside the tree, as sp1-prover stages it, so that du counts both.
    tarball=$SP1_PLONK_CIRCUIT_PATH/$PLONK_CIRCUIT_VERSION-plonk.tar.gz
    /usr/bin/time -f "download: %e s" curl -fsSL --retry 5 --retry-all-errors -o "$tarball" "$PLONK_CIRCUIT_URL"
    ls -l "$tarball"
    /usr/bin/time -f "extract: %e s" tar -Pxzf "$tarball" -C "$dir"
    notes+=("peak circuit disk before the step: $(($(du -sk "$SP1_PLONK_CIRCUIT_PATH" | cut -f1) / 1024)) MiB, the tarball and the extracted tree")
    echo "${notes[-1]}"
    rm "$tarball"
    # All circuit_artifacts_installed checks (sp1-prover 6.10.0 src/build.rs).
    touch "$dir/.complete"
    du -sh "$dir"
    plonk_step aggregate-n1-plonk aggregate --segment-elf "$segment_elf" --aggregator-elf "$aggregator_elf" \
        --proofs "$proofs" --tags B0L10 --mode plonk --out "$out/n1.plonk.bin" || failed+=(aggregate-n1-plonk)
    ;;
esac

{
    echo "===== BEGIN RESULTS ($mode) ====="
    git -C "$root" log -1 --format='commit %H' || true
    date -u +%Y-%m-%dT%H:%M:%SZ
    nvidia-smi --query-gpu=name,compute_cap,memory.total,driver_version,power.limit --format=csv
    for name in "${steps[@]}"; do
        log=$results/$name.log
        echo "--- $name (exit $(cat "$results/$name.rc" 2>/dev/null))"
        # sp1-host's own lines; the server's tracing lines stay in the job log above.
        grep -Ev '^ *[0-9]{4}-[0-9]{2}-[0-9]{2}T' "$log" | grep -E \
            '^(== |OK|PASS|FAIL|FAILED|skip|failed tags|honest half|B1:|echo|digest|executor|client build|setup|aggregator setup|prove time|stdin bytes|proof|fixture|sp1-gpu-server VmHWM|--then|uploaded)|^ *"(programVKey|publicValues|proofBytes)"' || true
        cat "$results/$name.gpu-peak" 2>/dev/null || true
        cat "$results/$name.disk-peak" 2>/dev/null || true
        cat "$results/$name.outcome" 2>/dev/null || true
        grep -E "Elapsed|Maximum resident" "$results/$name.time" 2>/dev/null || true
        # The server's stages, wherever its log names them.
        grep -E '^ *[0-9]{4}-[0-9]{2}-[0-9]{2}T' "$log" |
            grep -iE 'deferred|compress|shrink|wrap|plonk|gnark|circuit|artifact|download' | head -60 || true
    done
    for note in "${notes[@]}"; do echo "$note"; done
    echo "failed steps: ${failed[*]:-none}"
    echo "===== END RESULTS ($mode) ====="
} 2>&1
phase "done"
((${#failed[@]} == 0))
