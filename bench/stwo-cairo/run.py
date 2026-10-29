#!/usr/bin/env python3
"""S-two unit costs for BLAKE2s, QM31 and M31 ingestion (issue #39).

Reproduces every table of the "S-two unit costs" section of docs/STARK-NATIVE-ROUTE.md:
clones stwo-cairo at the pinned commit, builds its prover, executes and proves this
package's benchmarks for N = 2^12..2^18 with production parameters (`--verify`), runs the
two reference-verifier calibrations, fits fixed + per-op costs and extrapolates to
TinyLlama. A benchmark stops at the first N whose proof runs out of memory (--max-gib).
Neither `make check` nor CI runs it: a full run takes about an hour on an M3 Max. Runs on
macOS (`/usr/bin/time -l`) and Linux (GNU `/usr/bin/time -v`).

    python3 -I bench/stwo-cairo/run.py            # everything, then the report
    python3 -I bench/stwo-cairo/run.py --report   # only re-render the report from results

Needs scarb 2.18.0, rustup (stwo-cairo's rust-toolchain.toml selects nightly-2026-01-15) and,
on Linux, GNU time at /usr/bin/time (Debian/Ubuntu package `time`).
Writes the clone, proofs and logs under runs/ (gitignored) unless --stwo-cairo/--out say
otherwise.
"""

import argparse
import json
import math
import os
import platform
import re
import subprocess
import sys
import threading
import time
from pathlib import Path

STWO_CAIRO_URL = "https://github.com/starkware-libs/stwo-cairo.git"
STWO_CAIRO_REV = "2b1fb4ded5496fb2db4e8261cd5ee5ef6b710839"
SCARB_VERSION = "scarb 2.18.0"
HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
PACKAGE = "stwo_unit_costs"

BENCHES = {"blake2s": 0, "qm31": 1, "m31": 2}
LOG_NS = range(12, 19)
FIT_MIN_LOG_N = 14
M31_P = 2**31 - 1
FELT_P = 2**251 + 17 * 2**192 + 1
LOG_MEMORY_ADDRESS_BOUND = 29  # stwo-cairo crates/common/src/memory.rs:2
# Memory table limits at the pin: memory_address_to_id packs 2^(29 - 25) = 16 addresses per row
# (crates/cairo-air/src/components/memory_address_to_id.rs:29); memory_id_to_small holds at most
# 2^24 distinct values and the rest become big values (crates/adapter/src/memory.rs:66, 160-178);
# memory_id_to_big splits into tables of at most 2^25 rows (MAX_SEQUENCE_LOG_SIZE,
# crates/prover/src/witness/cairo_claim_generator.rs:1441).
ADDRESSES_PER_ROW = 16
LOG_SMALL_VALUE_CAPACITY = 24
LOG_MAX_BIG_SIZE = 25
# Base + interaction columns of the memory tables, whose sources compute them
# (memory_address_to_id.rs:32,82; memory_id_to_small.rs:7,27; memory_id_to_big.rs:23,237).
MEMORY_COLUMNS = {"memory_address_to_id": 32 + 32, "memory_id_to_small": 9 + 12,
                  "memory_id_to_big": 29 + 32}
# CI step counts of the reference verifiers (.github/workflows/cairo-ci.yaml:136-153).
EXPECTED_CALIBRATION_STEPS = {"stwo_cairo_verifier": 18_152_624, "stwo_circuit_verifier": 1_982_220}
QUERIES = 100  # SASH's segment proofs (prover/src/util.rs:14-16)

# Components a BLAKE2s compression adds rows to, with rows per compression, and the QM31 one.
# Every other component that grows with the program has one row per opcode or builtin instance.
BLAKE_COMPONENTS = {"blake_compress_opcode": 1, "blake_round": 10, "blake_g": 80, "triple_xor_32": 8}
QM31_COMPONENT = "qm_31_add_mul_opcode"

# TinyLlama-1.1B SASH run, docs/EVM-GAS-MODEL.md §1, and the BLAKE2s compressions its verifier
# runs, Merkle hash_node plus channel, counted natively (§3, §7).
TINYLLAMA = {
    "all 23 segments": {"segments": 23, "leaf_m31": 65_987_266, "oods": 519_784,
                        "columns": 501_235, "hashes": 518_617, "blake2s": 5_134_853},
    "head (largest)": {"segments": 1, "leaf_m31": 8_164_242, "oods": 69_560,
                       "columns": 68_484, "hashes": 19_534, "blake2s": 561_462},
}

# The executable's Standalone entrypoint passes only the builtins the program uses, but
# run_and_prove at this commit reads all 11 public-segment pointers (the adapter's
# bootloader context). We prepend a Standalone header that receives all 11 builtins in the
# canonical order and calls the executable's Bootloader entrypoint (a function taking and
# returning its own builtins), returning the untouched pointers for the unused ones.
ALL_BUILTINS = ["output", "pedersen", "range_check", "ecdsa", "bitwise", "ec_op", "keccak",
                "poseidon", "range_check96", "add_mod", "mul_mod"]
AP_ADD_IMM = "0x40780017fff7fff"
CALL_REL_IMM = "0x1104800180018000"
JMP_REL_IMM = "0x10780017fff7fff"
RET = "0x208b7fff7fff7ffe"


def push_fp(off):  # [ap + 0] = [fp + off], ap++
    return hex(0x480A00007FFF8000 | ((0x8000 + off) << 32))


def push_ap(off):  # [ap + 0] = [ap + off], ap++
    return hex(0x481200007FFF8000 | ((0x8000 + off) << 32))


def with_all_builtins(executable):
    code = executable["program"]["bytecode"]
    (boot,) = [e for e in executable["entrypoints"] if e["kind"] == "Bootloader"]
    used = boot["builtins"]
    assert used == [b for b in ALL_BUILTINS if b in used], used
    header_len, n = 6, len(ALL_BUILTINS)
    shim_at = header_len + len(code)
    header = [AP_ADD_IMM, hex(n), CALL_REL_IMM, hex(shim_at - 2), JMP_REL_IMM, "0x0"]
    # The 11 initial pointers sit at [fp - 13 + i] inside the shim's frame.
    shim = [push_fp(-(n + 2) + ALL_BUILTINS.index(b)) for b in used]
    call_pc = shim_at + len(shim)
    shim += [CALL_REL_IMM, hex((boot["offset"] + header_len - call_pc) % FELT_P)]
    for i, b in enumerate(ALL_BUILTINS):
        if b in used:  # its stop pointer, returned by the call
            shim.append(push_ap(-(i + len(used) - used.index(b))))
        else:  # unused: stop pointer = start pointer
            shim.append(push_fp(-(n + 2) + i))
    shim.append(RET)
    return {
        "program": {"bytecode": header + code + shim,
                    "hints": [[pc + header_len, h] for pc, h in executable["program"]["hints"]]},
        "entrypoints": [{"builtins": ALL_BUILTINS, "offset": 0, "kind": "Standalone"}],
    }


def run(cmd, cwd=None, env=None, check=True):
    print("+", " ".join(str(c) for c in cmd), flush=True)
    res = subprocess.run([str(c) for c in cmd], cwd=cwd, env=env, text=True,
                         capture_output=True)
    if check and res.returncode != 0:
        sys.exit(f"command failed ({res.returncode}): {cmd}\n{res.stdout}\n{res.stderr}")
    return res


def setup(stwo):
    if not (stwo / ".git").exists():
        stwo.mkdir(parents=True, exist_ok=True)
        run(["git", "init", "-q"], cwd=stwo)
        run(["git", "remote", "add", "origin", STWO_CAIRO_URL], cwd=stwo)
        run(["git", "fetch", "-q", "--depth", "1", "origin", STWO_CAIRO_REV], cwd=stwo)
        run(["git", "checkout", "-q", "FETCH_HEAD"], cwd=stwo)
    head = run(["git", "rev-parse", "HEAD"], cwd=stwo).stdout.strip()
    assert head == STWO_CAIRO_REV, f"{stwo} is at {head}, not {STWO_CAIRO_REV}"
    scarb = run(["scarb", "--version"]).stdout.splitlines()[0]
    assert scarb.startswith(SCARB_VERSION), f"need {SCARB_VERSION}, found {scarb}"
    env = dict(os.environ, RUSTFLAGS="-C target-cpu=native")
    run(["cargo", "build", "--release", "--locked", "-p", "stwo-cairo-dev-utils",
         "--bin", "run_and_prove", "--bin", "get_execution_resources"], cwd=stwo, env=env)
    run(["scarb", "build"], cwd=HERE)
    return {
        "rustc": run(["rustc", "--version"], cwd=stwo).stdout.strip(),
        "scarb": scarb,
        "stwo_cairo": head,
    }


MACOS = sys.platform == "darwin"


def sysctl(key):
    return run(["sysctl", "-n", key]).stdout.strip()


def cgroup(name):
    """A cgroup v2 limit of this process, or None if unlimited or absent."""
    f = Path("/sys/fs/cgroup") / name
    value = f.read_text().split() if f.exists() else ["max"]
    return None if value[0] == "max" else [int(x) for x in value]


def meminfo_kib(key):
    return int(re.search(rf"^{key}:\s+(\d+) kB", Path("/proc/meminfo").read_text(), re.M).group(1))


def ram_gib():
    """Memory a proof can use: physical RAM, or the cgroup limit if lower."""
    if MACOS:
        return int(sysctl("hw.memsize")) / 2**30
    limit = cgroup("memory.max")
    return min(meminfo_kib("MemTotal") * 1024, limit[0] if limit else math.inf) / 2**30


def machine():
    if MACOS:
        return {
            "cpu": sysctl("machdep.cpu.brand_string"),
            "cores": f"{sysctl('hw.ncpu')} ({sysctl('hw.perflevel0.physicalcpu')} performance"
                     f" + {sysctl('hw.perflevel1.physicalcpu')} efficiency)",
            "ram_gib": ram_gib(),
            "os": f"macOS {platform.mac_ver()[0]}",
        }
    info = Path("/proc/cpuinfo").read_text()
    blocks = [dict(re.findall(r"^([^\t:]+?)\s*: (.*)$", b, re.M)) for b in info.split("\n\n") if b.strip()]
    host_cores = len({(b.get("physical id"), b.get("core id")) for b in blocks})
    quota = cgroup("cpu.max")
    usable = len(os.sched_getaffinity(0))
    os_name = re.search(r'^PRETTY_NAME="?(.*?)"?$', Path("/etc/os-release").read_text(), re.M).group(1)
    return {
        "cpu": blocks[0].get("model name", platform.machine()),
        "cores": f"{usable} ("
                 + (f"cgroup quota {quota[0] / quota[1]:g} CPUs; " if quota else "")
                 + f"host: {len(blocks)} logical, {host_cores} physical)",
        "ram_gib": ram_gib(),
        "host_ram_gib": meminfo_kib("MemTotal") / 2**20,
        "os": f"{os_name}, Linux {platform.release()}",
        "flags": [f for f in ("avx2", "avx512f", "sha_ni") if f in blocks[0].get("flags", "").split()],
    }


def args_file(path, bench, n):
    values = [(i * 0x9E3779B1) % M31_P for i in range(1, n + 1)] if bench == "m31" else []
    felts = [BENCHES[bench], n, len(values), *values]
    path.write_text(json.dumps([hex(v) for v in felts]))


def resource_usage(out):
    """Steps and max memory address from `scarb execute --print-resource-usage`."""
    def field(name):
        return int(re.search(rf"{name}: ([\d,]+)", out).group(1).replace(",", ""))
    return {"steps": field("steps"), "max_memory_address": field("max memory address")}


def scarb_execute(args):
    return resource_usage(run(["scarb", "execute", "--no-build", "--print-resource-usage",
                               "--output", "none", "--arguments-file", args], cwd=HERE).stdout)


def swap_used_mib():
    if not MACOS:
        return (meminfo_kib("SwapTotal") - meminfo_kib("SwapFree")) / 1024
    out = run(["sysctl", "-n", "vm.swapusage"]).stdout
    return float(re.search(r"used = ([\d.]+)M", out).group(1))


def child_pid(pid):
    if MACOS:
        kids = subprocess.run(["pgrep", "-P", str(pid)], capture_output=True, text=True).stdout
    else:
        f = Path(f"/proc/{pid}/task/{pid}/children")
        kids = f.read_text() if f.exists() else ""
    return kids.split()[0] if kids.split() else None


def footprint_bytes(pid):
    """macOS: physical footprint, compressed and swapped-out memory included, as top's MEM shows
    it. Linux: resident set size."""
    if not MACOS:
        f = Path(f"/proc/{pid}/status")
        m = re.search(r"^VmRSS:\s+(\d+) kB", f.read_text(), re.M) if f.exists() else None
        return int(m.group(1)) * 1024 if m else 0
    out = subprocess.run(["top", "-l", "1", "-pid", str(pid), "-stats", "mem"],
                         capture_output=True, text=True).stdout.split()
    m = re.fullmatch(r"([\d.]+)([BKMGT])[+-]?", out[-1]) if out else None
    return float(m.group(1)) * 1024 ** "BKMGT".index(m.group(2)) if m else 0


def wall_seconds(text):
    if MACOS:
        return float(re.search(r"([\d.]+) real", text).group(1))
    clock = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([\d:.]+)", text).group(1)
    return sum(float(x) * 60**i for i, x in enumerate(reversed(clock.split(":"))))


ANSI = re.compile(r"\x1b\[[0-9;]*m")


def prove(bins, program, args, proof, log, max_gib, settle_s):
    """Proves and verifies with run_and_prove; returns timings and what the logs report."""
    cmd = ["/usr/bin/time", "-l" if MACOS else "-v", bins / "run_and_prove", "--program", program,
           "--program_type", "executable", "--program_arguments_file", args,
           "--proof_path", proof, "--verify"]
    for _ in range(settle_s // 10):  # timings: wait for the 1-minute load to settle below 3
        if os.getloadavg()[0] < 3:
            break
        time.sleep(10)
    print("+", " ".join(str(c) for c in cmd), flush=True)
    load, swap_before = os.getloadavg(), swap_used_mib()
    killed = []
    with open(log, "w") as f:
        p = subprocess.Popen([str(c) for c in cmd], stdout=f, stderr=subprocess.STDOUT,
                             env=dict(os.environ, RUST_LOG="info"))

        def watchdog():  # macOS compresses and swaps rather than failing an allocation
            while p.poll() is None:
                try:  # the prover may exit between these reads
                    kid = child_pid(p.pid)
                    if kid and footprint_bytes(kid) > max_gib * 2**30:
                        os.kill(int(kid), 9)
                        killed.append(True)
                        return
                except (FileNotFoundError, ProcessLookupError):
                    pass
                time.sleep(0.5)
        threading.Thread(target=watchdog, daemon=True).start()
        p.wait()
    text = ANSI.sub("", Path(log).read_text(errors="replace"))
    r = {"load_avg": [round(x, 2) for x in load], "swap_mib": [swap_before, swap_used_mib()]}
    if MACOS:
        for key, label in [("max_rss_b", "maximum resident set size"),
                           ("peak_footprint_b", "peak memory footprint")]:
            found = re.search(rf"(\d+)\s+{label}", text)
            if found:
                r[key] = int(found.group(1))
    else:
        found = re.search(r"Maximum resident set size \(kbytes\): (\d+)", text)
        if found:
            r["max_rss_b"] = int(found.group(1)) * 1024
    if killed:
        return dict(r, status=f"out of memory: {'footprint' if MACOS else 'RSS'} above "
                              f"{max_gib:g} GiB, stopped")
    if "terminated abnormally" in text:
        return dict(r, status=f"out of memory: killed by macOS at a peak footprint of "
                              f"{r['peak_footprint_b'] / 2**30:.1f} GiB")
    if "Command terminated by signal 9" in text:
        return dict(r, status=f"out of memory: killed at a peak RSS of "
                              f"{r['max_rss_b'] / 2**30:.1f} GiB")
    if "Proved successfully" not in text:
        return dict(r, status="failed, see " + str(log))
    cells = re.search(r"Witness trace cells: \[(\d+), (\d+), (\d+)\]", text).groups()
    uses = json.loads(re.search(r"All relation uses:\n(\{.*?\n\})", text, re.S).group(1))
    claim = json.loads(Path(proof).read_text())["claim"]
    log_sizes = {}
    for name, c in claim.items():
        if isinstance(c, dict) and "log_size" in c:
            log_sizes[name] = c["log_size"]
        elif isinstance(c, dict) and "big_log_sizes" in c:
            log_sizes[name] = c["big_log_sizes"]
    return dict(
        r,
        status="verified",
        prove_s=wall_seconds(text),
        prover_steps=int(re.search(r"Num steps: (\d+)", text).group(1)),
        cells={"preprocessed": int(cells[0]), "base": int(cells[1]), "interaction": int(cells[2])},
        relation_uses=uses,
        log_sizes=log_sizes,
        proof_bytes=Path(proof).stat().st_size,
    )


def execution_resources(bins, program, args, out):
    run([bins / "get_execution_resources", "--program", program, "--program_type",
         "executable", "--program_arguments_file", args, "--output", out])
    return json.loads(Path(out).read_text())


def component_columns(stwo, names):
    """Base and interaction columns of each component, read from the pinned sources."""
    cols = {}
    for name in names:
        if name in MEMORY_COLUMNS:
            cols[name] = {"base": 0, "interaction": MEMORY_COLUMNS[name]}
            continue
        src = (stwo / "crates/cairo-air/src/components" / f"{name}.rs").read_text()
        base = int(re.search(r"N_TRACE_COLUMNS: usize = (\d+);", src).group(1))
        inter = re.search(r"SECURE_EXTENSION_DEGREE(?: \* (\d+))?\]", src).group(1)
        cols[name] = {"base": base, "interaction": 4 * int(inter or 1)}
    return cols


def component_rows(res):
    """Unpadded rows of each component that grows with the program, from get_execution_resources."""
    rows = dict(res["opcodes_instance_counter"])
    for name, per in BLAKE_COMPONENTS.items():
        rows[name] = per * rows.get("blake_compress_opcode", 0)
    rows.update({k: v for k, v in res["builtin_instance_counter"].items() if k != "output_builtin"})
    mem = res["memory_tables_sizes"]
    rows["memory_address_to_id"] = math.ceil(mem["memory_address_to_id"] / ADDRESSES_PER_ROW)
    rows["memory_id_to_small"] = min(mem["memory_id_to_small"], 2**LOG_SMALL_VALUE_CAPACITY)
    big = mem["memory_id_to_big"] + mem["memory_id_to_small"] - rows["memory_id_to_small"]
    rows["memory_id_to_big"] = [min(2**LOG_MAX_BIG_SIZE, big - i)
                                for i in range(0, max(big, 1), 2**LOG_MAX_BIG_SIZE)]
    rows["verify_instruction"] = res["verify_instruction"]
    return rows


def padded(n):  # a component's rows, padded to a power of two (at least one SIMD vector)
    return 2 ** max(4, math.ceil(math.log2(n))) if n > 0 else 0


def growing_cells(rows, cols, pad):
    """Base + interaction cells of the components in `rows`, padded or not."""
    f = padded if pad else (lambda n: n)
    return sum(sum(f(n) for n in (v if isinstance(v, list) else [v]))
               * (cols[c]["base"] + cols[c]["interaction"]) for c, v in rows.items())


def measure(stwo, bins, out, benches, log_ns, max_gib, settle_s):
    executable = HERE / "target/dev" / f"{PACKAGE}.executable.json"
    program = out / f"{PACKAGE}.all_builtins.executable.json"
    program.write_text(json.dumps(with_all_builtins(json.loads(executable.read_text()))))
    results = out / "results.jsonl"
    for bench in benches:
        for log_n in log_ns:
            n = 2**log_n
            tag = f"{bench}-{log_n}"
            args = out / f"{tag}.args.json"
            args_file(args, bench, n)
            row = {"bench": bench, "log_n": log_n, "n": n, **scarb_execute(args)}
            row["resources"] = execution_resources(bins, program, args, out / f"{tag}.resources.json")
            row.update(prove(bins, program, args, out / f"{tag}.proof.json", out / f"{tag}.log",
                             max_gib, settle_s))
            with open(results, "a") as f:
                f.write(json.dumps(row) + "\n")
            print(f"{tag}: {row['status']} {row.get('prove_s')} s", flush=True)
            if row["status"] != "verified":
                break  # larger N needs more memory still: stop this benchmark here


def calibration_runs(stwo, bins, out):
    """Executes the two reference verifiers with the CI commands; returns their rows and the
    stwo_cairo_verifier program to prove."""
    ver = stwo / "stwo_cairo_verifier"
    rows = []
    for package, proof in [
        ("stwo_cairo_verifier", "../test_data/test_prove_verify_all_opcode_components/proof.json"),
        ("stwo_circuit_verifier", "crates/circuit_air/test_data/proof.json"),
    ]:
        usage = resource_usage(run(
            ["scarb", "--profile", "proving", "execute", "--package", package, "--features",
             "qm31_opcode", "--print-resource-usage", "--output", "none", "--arguments-file",
             proof], cwd=ver).stdout)
        assert usage["steps"] == EXPECTED_CALIBRATION_STEPS[package], (package, usage)
        rows.append({"package": package, **usage, "arguments": proof,
                     "argument_felts": len(json.loads((ver / proof).read_text()))})
    executable = ver / "target/proving/stwo_cairo_verifier.executable.json"
    program = out / "stwo_cairo_verifier.all_builtins.executable.json"
    program.write_text(json.dumps(with_all_builtins(json.loads(executable.read_text()))))
    rows[0]["resources"] = execution_resources(bins, program, ver / rows[0]["arguments"],
                                               out / "calibration.resources.json")
    return rows, program


def calibrate(stwo, bins, out, max_gib, settle_s):
    rows, program = calibration_runs(stwo, bins, out)
    ver = stwo / "stwo_cairo_verifier"
    rows[0].update(prove(bins, program, ver / rows[0]["arguments"],
                         out / "calibration.proof.json", out / "calibration.log", max_gib,
                         settle_s))
    (out / "calibration.json").write_text(json.dumps(rows, indent=1))


# ---------------------------------------------------------------------------- report


def fit(points):
    """Least squares y = a + b * n over the points at n >= 2^FIT_MIN_LOG_N; (a, b, residuals)."""
    pts = [(n, y) for n, y in points if n >= 2**FIT_MIN_LOG_N]
    if len(pts) < 3:
        return None
    mn = sum(n for n, _ in pts) / len(pts)
    my = sum(y for _, y in pts) / len(pts)
    b = sum((n - mn) * (y - my) for n, y in pts) / sum((n - mn) ** 2 for n, _ in pts)
    a = my - b * mn
    return a, b, [y - a - b * n for n, y in pts]


def trace_cells(r):
    return r["cells"]["base"] + r["cells"]["interaction"]


def padded_rows(r, name):
    v = r["log_sizes"].get(name)
    if v is None:
        return 0
    return sum(2**x for x in v) if isinstance(v, list) else 2**v


def total_rows(v):
    return sum(v) if isinstance(v, list) else v


def fmt(x, digits=0):
    return f"{x:,.{digits}f}"


def loads(r):
    return "/".join(f"{x:.1f}" for x in r["load_avg"])


def report(stwo, out):
    meta = json.loads((out / "meta.json").read_text())
    rows = [json.loads(line) for line in (out / "results.jsonl").read_text().splitlines()]
    rows = list({(r["bench"], r["n"]): r for r in rows}.values())  # last run of each point wins
    cal_path = out / "calibration.json"
    cal = json.loads(cal_path.read_text()) if cal_path.exists() else []
    names = {c for r in rows for c in component_rows(r["resources"])}
    cols = component_columns(stwo, names | set(BLAKE_COMPONENTS) | {QM31_COMPONENT})
    lines = []
    w = lines.append
    m = meta["machine"]
    macos = m["os"].startswith("macOS")
    w(f"Machine: {m['cpu']}, {m['cores']} cores, {m['ram_gib']:.0f} GiB"
      + (f" (host {m['host_ram_gib']:.0f} GiB)" if "host_ram_gib" in m else "") + f", {m['os']}"
      + (f", CPU flags {', '.join(m['flags'])}" if m.get("flags") else "") + ".")
    w(f"Toolchains: {meta['versions']['scarb']}; {meta['versions']['rustc']}; "
      f"stwo-cairo {meta['versions']['stwo_cairo']}.")
    w("")
    w(f"Time is {'`/usr/bin/time -l`' if macos else 'GNU `/usr/bin/time -v`'} wall time of the "
      "whole `run_and_prove` command: Cairo VM run, adaptation, proving, verification and "
      "writing the proof." + ("" if macos else " Linux reports no peak footprint: peak RSS is the "
                              "figure that has to fit in RAM."))
    w("")

    # Every verified run: the growing components' rows from get_execution_resources must give the
    # claim's log sizes, and the remaining (fixed-size) components a constant number of cells.
    fixed_cells = {}
    for r in [*rows, *cal[:1]]:
        if r.get("status") != "verified":
            continue
        grow = component_rows(r["resources"])
        for c, v in grow.items():
            want = [2**x for x in r["log_sizes"][c]] if isinstance(r["log_sizes"].get(c), list) \
                else [padded_rows(r, c)]
            got = [padded(n) for n in (v if isinstance(v, list) else [v]) if n]
            assert got == [x for x in want if x], (r.get("bench", "calibration"), c, v, want)
        fixed_cells.setdefault(r.get("bench", "calibration"), set()).add(
            trace_cells(r) - growing_cells(grow, cols, pad=True))
    assert all(len(v) == 1 for v in fixed_cells.values()), fixed_cells
    fixed_cells = {k: v.pop() for k, v in fixed_cells.items()}

    fits = {}
    for bench in BENCHES:
        br = sorted((r for r in rows if r["bench"] == bench), key=lambda r: r["n"])
        if not br:
            continue
        w(f"### {bench}")
        w("")
        w("| N | Steps (`scarb execute`) | Max memory address | Trace cells (base + interaction) "
          "| Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) "
          "| Load avg (1/5/15 min) | Status |")
        w("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|")
        for r in br:
            ok = r["status"] == "verified"
            w(f"| 2^{r['log_n']} | {fmt(r['steps'])} | {fmt(r['max_memory_address'])} | "
              + (f"{fmt(trace_cells(r))} | {trace_cells(r) / r['steps']:,.0f} | {r['prove_s']:.2f} | "
                 f"{fmt(r['steps'] / r['prove_s'])} | {r['max_rss_b'] / 2**30:.1f} | "
                 if ok else "— | — | — | — | — | ")
              + (f"{r['peak_footprint_b'] / 2**30:.1f} | " if "peak_footprint_b" in r else "— | ")
              + f"{loads(r)} | {r['status']} |")
        ok = [r for r in br if r["status"] == "verified"]
        f = {
            "steps": fit([(r["n"], r["steps"]) for r in br]),
            "memory": fit([(r["n"], r["max_memory_address"]) for r in br]),
            "cells": fit([(r["n"], trace_cells(r)) for r in ok]),
            "prove_s": fit([(r["n"], r["prove_s"]) for r in ok]),
            "rss_gib": fit([(r["n"], r["max_rss_b"] / 2**30) for r in ok]),
            "footprint_gib": fit([(r["n"], r["peak_footprint_b"] / 2**30) for r in ok])
            if all("peak_footprint_b" in r for r in ok) else None,
        }
        uses = {}
        for rel in ok[-1]["relation_uses"] if ok else []:
            uses[rel] = fit([(r["n"], r["relation_uses"].get(rel, 0)) for r in ok])
        grow = {}
        for name in {c for r in br for c in component_rows(r["resources"])}:
            grow[name] = fit([(r["n"], total_rows(component_rows(r["resources"]).get(name, 0)))
                              for r in br])
        a, b = ok[-2], ok[-1]
        marginal = (b["prove_s"] - a["prove_s"]) / (b["n"] - a["n"])
        fits[bench] = dict(f, relation_uses=uses, rows=grow, marginal_s=marginal)
        w("")
        w("| Fit over N ≥ 2^14 (" + ", ".join(f"2^{r['log_n']}" for r in ok
                                              if r["log_n"] >= FIT_MIN_LOG_N) + ") | Fixed | Per op |")
        w("|---|---:|---:|")
        for key, label, unit_fixed, unit_op, scale in [
                ("steps", "Steps", "", "", 1), ("memory", "Memory addresses", "", "", 1),
                ("cells", "Trace cells, padded at N = 2^k (base + interaction)", "", "", 1),
                ("prove_s", "Time", " s", " µs", 1e6),
                ("rss_gib", "Peak RSS", " GiB", " KiB", 2**20),
                ("footprint_gib", "Peak footprint", " GiB", " KiB", 2**20)]:
            if f[key]:
                d = 2 if unit_fixed else 0
                w(f"| {label} | {f[key][0]:,.{d}f}{unit_fixed} | {f[key][1] * scale:,.2f}{unit_op} |")
        w(f"| Time, marginal from 2^{a['log_n']} to 2^{b['log_n']} | | {marginal * 1e6:,.2f} µs |")
        w("| Time fit residuals (s) | " + ", ".join(f"{x:+.2f}" for x in f["prove_s"][2]) + " | |")
        w(f"| Cells per step, marginal (fit) | | {f['cells'][1] / f['steps'][1]:,.0f} |")
        w("")
        opc = ok[-1]["resources"]["opcodes_instance_counter"]
        per_op = {k: v / ok[-1]["n"] for k, v in opc.items() if v / ok[-1]["n"] >= 0.001}
        w(f"Opcode instances per op at N = 2^{ok[-1]['log_n']}: "
          + ", ".join(f"`{k}` {v:.4f}" for k, v in sorted(per_op.items(), key=lambda x: -x[1])) + ".")
        w("")

    # Cells per BLAKE2s compression and per QM31 op, next to the inferred 99 rows.
    blake = [r for r in rows if r["bench"] == "blake2s" and r["status"] == "verified"]
    if blake:
        r = blake[-1]
        n = r["n"]
        count = r["resources"]["opcodes_instance_counter"]["blake_compress_opcode"]
        w(f"### Rows and cells per BLAKE2s compression (N = 2^{r['log_n']}, "
          f"{fmt(count)} `blake_compress_opcode` instances)")
        w("")
        w("| Component | Columns (base + interaction) | Rows, inferred | Rows, padded (log size) "
          "| Cells, unpadded | Cells, padded |")
        w("|---|---:|---:|---:|---:|---:|")
        tot = [0, 0, 0, 0]
        for name, per in BLAKE_COMPONENTS.items():
            c = cols[name]["base"] + cols[name]["interaction"]
            pad = padded_rows(r, name) / n
            unpadded = count * per / n
            tot = [tot[0] + per, tot[1] + pad, tot[2] + unpadded * c, tot[3] + pad * c]
            w(f"| `{name}` | {cols[name]['base']} + {cols[name]['interaction']} | {per} | "
              f"{pad:g} (2^{r['log_sizes'][name]}) | {fmt(unpadded * c)} | {fmt(pad * c)} |")
        w(f"| **Total** | | **{tot[0]}** | **{tot[1]:g}** | **{fmt(tot[2])}** | **{fmt(tot[3])}** |")
        w("")
    qm = [r for r in rows if r["bench"] == "qm31" and r["status"] == "verified"]
    if qm:
        r = qm[-1]
        c = cols[QM31_COMPONENT]
        count = r["resources"]["opcodes_instance_counter"][QM31_COMPONENT]
        w(f"QM31 fused mul-add (N = 2^{r['log_n']}): {count / r['n']:g} `qm_31_add_mul_opcode` rows "
          f"per op, {c['base']} + {c['interaction']} columns: "
          f"{fmt(count / r['n'] * (c['base'] + c['interaction']))} cells unpadded, "
          f"{fmt(padded_rows(r, QM31_COMPONENT) / r['n'] * (c['base'] + c['interaction']))} "
          f"padded (2^{r['log_sizes'][QM31_COMPONENT]} rows).")
        w("")
    w("Fixed-size components (lookup tables), base + interaction cells: "
      + ", ".join(f"{k} {fmt(v)}" for k, v in fixed_cells.items()) + ".")
    w("")

    if cal:
        w("### Calibration")
        w("")
        w("| Program | Steps | Max memory address | Time (s) | Steps/s | Trace cells (base + "
          "interaction) | Cells/step | Peak RSS (GiB) | Peak footprint (GiB) | Load avg | Status |")
        w("|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|")
        for c in cal:
            head = f"| `{c['package']}` | {fmt(c['steps'])} | {fmt(c['max_memory_address'])} | "
            if "status" not in c:
                w(head + "executed only | | | | | | | |")
            elif c["status"] != "verified":
                w(head + "— | — | — | — | — | "
                  + (f"{c['peak_footprint_b'] / 2**30:.1f}" if "peak_footprint_b" in c else "—")
                  + " | "
                  f"{loads(c)} | {c['status']} |")
            else:
                w(head + f"{c['prove_s']:.1f} | {fmt(c['steps'] / c['prove_s'])} | "
                  f"{fmt(trace_cells(c))} | {trace_cells(c) / c['steps']:,.0f} | "
                  f"{c['max_rss_b'] / 2**30:.1f} | "
                  + (f"{c['peak_footprint_b'] / 2**30:.1f}" if "peak_footprint_b" in c else "—")
                  + " | "
                  f"{loads(c)} | verified |")
        w("")
        w("Whole-run cells per step at each benchmark's largest verified N: " + ", ".join(
            f"{b} {trace_cells(r) / r['steps']:,.0f}" for b in BENCHES
            for r in [max((x for x in rows if x["bench"] == b and x["status"] == "verified"),
                          key=lambda x: x["n"])]) + ".")
        w("")

    if all(b in fits for b in BENCHES):
        extrapolate(fits, cols, fixed_cells, cal[0] if cal and cal[0].get("status") == "verified"
                    else None, rows, w)
    text = "\n".join(lines) + "\n"
    (out / "report.md").write_text(text)
    print(text)


class Model:
    """Fixed + per-op costs for a mix of the three ops. A proof pays the largest fixed cost once."""

    def __init__(self, fits, ops, segments=1):
        self.fits, self.ops, self.segments = fits, ops, segments

    def per_op(self, key, slope=None):
        return {b: (slope(b) if slope else self.fits[b][key][1]) * self.ops[b] for b in BENCHES}

    def fixed(self, key):
        return self.segments * max(self.fits[b][key][0] for b in BENCHES)

    def total(self, key, slope=None):
        return self.fixed(key) + sum(self.per_op(key, slope).values())

    def combined(self, kind):  # per relation (uses) or component (rows)
        acc = {}
        for name in {n for b in BENCHES for n, f in self.fits[b][kind].items() if f}:
            fs = [(b, self.fits[b][kind][name]) for b in BENCHES if self.fits[b][kind].get(name)]
            acc[name] = max(0, self.segments * max(f[0] for _, f in fs)
                            + sum(f[1] * self.ops[b] for b, f in fs))
        return acc

    def rows(self):
        """Growing components' rows, with the memory tables' limits applied as the adapter does."""
        rows = self.combined("rows")
        small = rows["memory_id_to_small"]
        rows["memory_id_to_small"] = min(small, 2**LOG_SMALL_VALUE_CAPACITY)
        big = rows["memory_id_to_big"] + small - rows["memory_id_to_small"]
        rows["memory_id_to_big"] = [min(2**LOG_MAX_BIG_SIZE, big - i)
                                    for i in range(0, max(math.ceil(big), 1), 2**LOG_MAX_BIG_SIZE)]
        return rows


def tinyllama_ops(t):
    return {
        "blake2s": t["blake2s"],
        # One step per query position and (column, sample) pair; every two-sample column also
        # gets a periodicity sample, and no column has more than two (as #38 models it).
        "qm31": QUERIES * (t["oods"] + (t["oods"] - t["columns"])),
        "m31": t["leaf_m31"] + 4 * t["oods"] + 8 * t["hashes"],
    }


def extrapolate(fits, cols, fixed_cells, cal, rows, w):
    # Time per padded trace cell of the largest proof measured: smaller proofs are dominated by
    # the fixed preprocessed tree, which the witness cells leave out.
    proofs = [r for r in rows if r["status"] == "verified"] + ([cal] if cal else [])
    big = max(proofs, key=trace_cells)
    ns_cell = big["prove_s"] / trace_cells(big)
    fixed_tables = max(fixed_cells.values())

    ratio, rel_ratio = {}, {}
    if cal:
        opc = cal["resources"]["opcodes_instance_counter"]
        cal_ops = {"blake2s": opc["blake_compress_opcode"], "qm31": opc[QM31_COMPONENT] / 2,
                   "m31": cal["argument_felts"]}
        cm = Model(fits, cal_ops)
        uses, model_rel = cal["relation_uses"], cm.combined("relation_uses")
        cal_rel = max(uses, key=uses.get)
        checks = [("Steps", cal["steps"], cm.total("steps")),
                  ("Max memory address", cal["max_memory_address"], cm.total("memory")),
                  (f"`{cal_rel}` uses", uses[cal_rel], model_rel[cal_rel]),
                  ("Trace cells", trace_cells(cal), cm.total("cells")),
                  ("Time (s)", cal["prove_s"], cm.total("prove_s"))]
        ratio = {name: measured / model for name, measured, model in checks}
        rel_ratio = {k: v / model_rel[k] for k, v in uses.items() if model_rel.get(k)}
        w("### Model against the calibration verifier")
        w("")
        w(f"`stwo_cairo_verifier`'s ops: {fmt(cal_ops['blake2s'])} BLAKE2s compressions, "
          f"{fmt(cal_ops['qm31'])} QM31 mul-adds (half its `qm_31_add_mul_opcode` instances), "
          f"{fmt(cal_ops['m31'])} argument felts.")
        w("")
        w("| | Measured | Modelled from the three ops | Measured / modelled |")
        w("|---|---:|---:|---:|")
        for name, measured, model in checks:
            d = 1 if name.startswith("Time") else 0
            w(f"| {name} | {fmt(measured, d)} | {fmt(model, d)} | {measured / model:.2f} |")
        w("")

    w("### Extrapolation to TinyLlama (estimate)")
    w("")
    w("Op counts: BLAKE2s compressions as SASH's verifier runs them, counted natively "
      f"(`docs/EVM-GAS-MODEL.md` §3); QM31 mul-adds = {QUERIES} query positions × "
      "(OODS values + two-sample columns), with two-sample columns = OODS values − columns; "
      "ingested felts = leaf M31s + 4 × OODS values + 8 × internal hashes. Costs add linearly; "
      "each segment is one proof, so it pays once the largest of the three fixed costs.")
    w("")
    w("| Workload | BLAKE2s | QM31 mul-adds | Ingested felts | Steps | Trace cells, unpadded "
      "| Trace cells, padded | Time, fit: BLAKE2s / QM31 / ingestion / fixed (s) | Time, fit (s) "
      "| Time, marginal slopes (s) | Time, cells × ns/cell of the largest proof (s) "
      "| Time, fit × measured / modelled on the calibration (s) |")
    w("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    head, rates = None, []
    for label, t in TINYLLAMA.items():
        ops = tinyllama_ops(t)
        md = Model(fits, ops, t["segments"])
        grow = md.rows()
        unpadded = t["segments"] * fixed_tables + growing_cells(grow, cols, pad=False)
        pad = fixed_tables + growing_cells(grow, cols, pad=True) if t["segments"] == 1 else None
        split = md.per_op("prove_s")
        t_fit = md.total("prove_s")
        t_marg = md.total("prove_s", slope=lambda b: fits[b]["marginal_s"])
        t_cells = (pad or unpadded) * ns_cell
        w(f"| {label} | {fmt(ops['blake2s'])} | {fmt(ops['qm31'])} | {fmt(ops['m31'])} | "
          f"{fmt(md.total('steps'))} | {unpadded:.3g} | " + (f"{pad:.3g}" if pad else "—") + " | "
          + " / ".join(fmt(split[b]) for b in BENCHES) + f" / {fmt(md.fixed('prove_s'))} | "
          f"{fmt(t_fit)} | {fmt(t_marg)} | {fmt(t_cells)} | "
          + (f"{fmt(t_fit * ratio['Time (s)'])} |" if ratio else "— |"))
        rates.append(f"{label} {md.total('steps') / t_fit:,.0f} steps/s and "
                     f"{t_fit / (pad or unpadded) * 1e9:.1f} ns per "
                     f"{'padded' if pad else 'unpadded'} cell")
        if t["segments"] == 1:
            head = (label, md, grow, pad, t_fit)
    w("")
    w(f"Largest proof measured: {big.get('package') or big['bench'] + ' 2^' + str(big['log_n'])}, "
      f"{ns_cell * 1e9:.1f} ns per padded trace cell. Per-inference cells "
      "are unpadded: the segments' own op counts are not modelled. The fit-based times imply "
      + "; ".join(rates) + ".")
    w("")

    label, md, grow, pad, t_fit = head
    rel = md.combined("relation_uses")
    top_rel = max(rel, key=rel.get)
    sizes = {c: max(math.log2(padded(n)) for n in (v if isinstance(v, list) else [v]))
             for c, v in grow.items() if total_rows(v) > 0}
    top = max(sizes.values())
    top_comp = sorted(c for c, s in sizes.items() if s == top)
    mem, steps = md.total("memory"), md.total("steps")
    r = ratio
    w(f"Checks for the largest segment, {label}:")
    w("")
    w("| Check | Modelled ops only | Scaled by measured / modelled on the calibration | Bound |")
    w("|---|---:|---:|---|")

    def scaled(x, key, digits=""):
        return f"{x * r[key]:,.0f}" + digits.format(math.log2(x * r[key])) if key in r else "—"
    w(f"| Memory addresses (execution + ingested arguments) | {fmt(mem)} (2^{math.log2(mem):.2f}) | "
      f"{scaled(mem, 'Max memory address', ' (2^{:.2f})')} | "
      f"2^{LOG_MEMORY_ADDRESS_BOUND} = {fmt(2**LOG_MEMORY_ADDRESS_BOUND)} |")
    rr = rel_ratio.get(top_rel)
    w(f"| Largest relation total uses, `{top_rel}` | {fmt(rel[top_rel])} ({rel[top_rel] / M31_P:.2f} P) | "
      + (f"{fmt(rel[top_rel] * rr)} ({rel[top_rel] * rr / M31_P:.2f} P)" if rr else "—")
      + f" | P = {fmt(M31_P)} |")
    w(f"| Steps | {fmt(steps)} (2^{math.log2(steps):.2f}) | {scaled(steps, 'Steps', ' (2^{:.2f})')} | |")
    w(f"| Largest component | " + ", ".join(f"`{c}`" for c in top_comp)
      + f", log size {top:.0f} | — | |")
    w(f"| Trace cells, padded | {pad:.3g} | "
      + (f"{pad * r['Trace cells']:.3g}" if r else "—") + " | |")
    w(f"| Time, fit (s) | {fmt(t_fit)} | {scaled(t_fit, 'Time (s)')} | |")
    mem_key = "footprint_gib" if md.fits["blake2s"]["footprint_gib"] else "rss_gib"
    w(f"| Peak {'footprint' if mem_key == 'footprint_gib' else 'RSS'} (GiB) | "
      f"{fmt(md.total(mem_key))} | — | |")
    w("")
    w("Growing components of the largest segment, rows (log size): " + ", ".join(
        f"`{c}` {fmt(total_rows(grow[c]))} ({s:.0f})"
        for c, s in sorted(sizes.items(), key=lambda x: (-x[1], x[0])) if s >= top - 3) + ".")
    w("")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--stwo-cairo", type=Path, default=REPO / "runs/stwo-cairo")
    ap.add_argument("--out", type=Path, default=REPO / "runs/stwo-unit-costs")
    ap.add_argument("--bench", choices=BENCHES, action="append")
    ap.add_argument("--log-n", type=int, action="append", help="default: 12..18")
    ap.add_argument("--max-gib", type=float, default=ram_gib() - 1,
                    help="stop a proof whose memory footprint (macOS) or RSS (Linux) passes "
                         "this: out of memory (default: RAM - 1 GiB)")
    ap.add_argument("--settle-s", type=int, default=180,
                    help="wait up to this long before each proof for the 1-minute load to fall "
                         "below 3")
    ap.add_argument("--skip-benchmarks", action="store_true")
    ap.add_argument("--skip-calibration", action="store_true")
    ap.add_argument("--report", action="store_true", help="only re-render the report")
    a = ap.parse_args()
    stwo, out = a.stwo_cairo.resolve(), a.out.resolve()
    if not a.report:
        out.mkdir(parents=True, exist_ok=True)
        versions = setup(stwo)
        (out / "meta.json").write_text(json.dumps({"machine": machine(), "versions": versions}))
        bins = stwo / "target/release"
        if not a.skip_benchmarks:
            measure(stwo, bins, out, a.bench or list(BENCHES), a.log_n or list(LOG_NS),
                    a.max_gib, a.settle_s)
        if not a.skip_calibration:
            calibrate(stwo, bins, out, a.max_gib, a.settle_s)
    report(stwo, out)


if __name__ == "__main__":
    main()
