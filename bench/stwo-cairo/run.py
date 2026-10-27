#!/usr/bin/env python3
"""S-two unit costs for BLAKE2s, QM31 and M31 ingestion (issue #39).

Reproduces every table of the "S-two unit costs" section of docs/STARK-NATIVE-ROUTE.md:
clones stwo-cairo at the pinned commit, builds its prover, executes and proves this
package's benchmarks for N = 2^12..2^18 with production parameters (`--verify`), runs the
two reference-verifier calibrations, fits fixed + per-op costs and extrapolates to
TinyLlama. A benchmark stops at the first N whose proof runs out of memory (--max-gib).
Neither `make check` nor CI runs it: a full run takes about an hour on an M3 Max.

    python3 -I bench/stwo-cairo/run.py            # everything, then the report
    python3 -I bench/stwo-cairo/run.py --report   # only re-render the report from results

Needs scarb 2.18.0 and rustup (stwo-cairo's rust-toolchain.toml selects nightly-2026-01-15).
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

# Components a BLAKE2s compression adds rows to, with rows per compression, and the QM31 one.
BLAKE_COMPONENTS = {"blake_compress_opcode": 1, "blake_round": 10, "blake_g": 80, "triple_xor_32": 8}
QM31_COMPONENT = "qm_31_add_mul_opcode"

# TinyLlama-1.1B SASH run, docs/EVM-GAS-MODEL.md §1 and §3.
TINYLLAMA = {
    "all 23 segments": {"segments": 23, "leaf_m31": 65_967_346, "oods": 519_784,
                        "columns": 501_197, "hashes": 518_489},
    "head (largest)": {"segments": 1, "leaf_m31": 8_099_813, "oods": 69_560,
                       "columns": 68_484, "hashes": 19_427},
}
TINYLLAMA_COMPRESSIONS = 4_641_448  # docs/EVM-GAS-MODEL.md §3

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


def machine():
    def sysctl(key):
        return run(["sysctl", "-n", key]).stdout.strip()
    return {
        "cpu": sysctl("machdep.cpu.brand_string"),
        "cores": f"{sysctl('hw.ncpu')} ({sysctl('hw.perflevel0.physicalcpu')} performance"
                 f" + {sysctl('hw.perflevel1.physicalcpu')} efficiency)",
        "ram_gib": int(sysctl("hw.memsize")) / 2**30,
        "os": f"macOS {platform.mac_ver()[0]}",
    }


def args_file(path, bench, n):
    values = [(i * 0x9E3779B1) % M31_P for i in range(1, n + 1)] if bench == "m31" else []
    felts = [BENCHES[bench], n, len(values), *values]
    path.write_text(json.dumps([hex(v) for v in felts]))


def scarb_execute(args):
    out = run(["scarb", "execute", "--no-build", "--print-resource-usage", "--output", "none",
               "--arguments-file", args], cwd=HERE).stdout

    def field(name):
        return int(re.search(rf"{name}: ([\d,]+)", out).group(1).replace(",", ""))
    return {"steps": field("steps"), "max_memory_address": field("max memory address")}


def swap_used_mib():
    out = run(["sysctl", "-n", "vm.swapusage"]).stdout
    return float(re.search(r"used = ([\d.]+)M", out).group(1))


def footprint_bytes(pid):
    """Physical footprint, compressed and swapped-out memory included, as top's MEM shows it."""
    out = subprocess.run(["top", "-l", "1", "-pid", str(pid), "-stats", "mem"],
                         capture_output=True, text=True).stdout.split()
    m = re.fullmatch(r"([\d.]+)([BKMGT])[+-]?", out[-1]) if out else None
    return float(m.group(1)) * 1024 ** "BKMGT".index(m.group(2)) if m else 0


ANSI = re.compile(r"\x1b\[[0-9;]*m")


def prove(bins, program, args, proof, log, max_gib):
    """Proves and verifies with run_and_prove; returns timings and what the logs report."""
    cmd = ["/usr/bin/time", "-l", bins / "run_and_prove", "--program", program,
           "--program_type", "executable", "--program_arguments_file", args,
           "--proof_path", proof, "--verify"]
    for _ in range(18):  # timings: wait up to 3 min for the 1-minute load to settle below 3
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
                kids = subprocess.run(["pgrep", "-P", str(p.pid)], capture_output=True,
                                      text=True).stdout.split()
                if kids and footprint_bytes(kids[0]) > max_gib * 2**30:
                    killed.append(True)
                    subprocess.run(["kill", "-9", kids[0]])
                    return
                time.sleep(0.5)
        threading.Thread(target=watchdog, daemon=True).start()
        p.wait()
    text = ANSI.sub("", Path(log).read_text(errors="replace"))
    r = {"load_avg": [round(x, 2) for x in load], "swap_mib": [swap_before, swap_used_mib()]}
    for key, label in [("max_rss_b", "maximum resident set size"),
                       ("peak_footprint_b", "peak memory footprint")]:
        found = re.search(rf"(\d+)\s+{label}", text)
        if found:
            r[key] = int(found.group(1))
    if killed:
        return dict(r, status=f"out of memory: footprint above {max_gib:g} GiB, stopped")
    if "terminated abnormally" in text:
        return dict(r, status=f"out of memory: killed by macOS at a peak footprint of "
                              f"{r['peak_footprint_b'] / 2**30:.1f} GiB")
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
        prove_s=float(re.search(r"([\d.]+) real", text).group(1)),
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


def component_columns(stwo):
    """Trace + interaction columns of each component, read from the pinned sources."""
    cols = {}
    for name in [*BLAKE_COMPONENTS, QM31_COMPONENT]:
        src = (stwo / "crates/cairo-air/src/components" / f"{name}.rs").read_text()
        base = int(re.search(r"N_TRACE_COLUMNS: usize = (\d+);", src).group(1))
        inter = int(re.search(r"SECURE_EXTENSION_DEGREE \* (\d+)\]", src).group(1)) * 4
        cols[name] = {"base": base, "interaction": inter}
    return cols


def measure(stwo, bins, out, benches, log_ns, max_gib):
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
                             max_gib))
            with open(results, "a") as f:
                f.write(json.dumps(row) + "\n")
            print(f"{tag}: {row['status']} {row.get('prove_s')} s", flush=True)
            if row["status"] != "verified":
                break  # larger N needs more memory still: stop this benchmark here


def calibrate(stwo, bins, out, max_gib):
    ver = stwo / "stwo_cairo_verifier"
    rows = []
    for package, proof in [
        ("stwo_cairo_verifier", "../test_data/test_prove_verify_all_opcode_components/proof.json"),
        ("stwo_circuit_verifier", "crates/circuit_air/test_data/proof.json"),
    ]:
        res = run(["scarb", "--profile", "proving", "execute", "--package", package, "--features",
                   "qm31_opcode", "--print-resource-usage", "--output", "none", "--arguments-file",
                   proof], cwd=ver).stdout
        steps = int(re.search(r"steps: ([\d,]+)", res).group(1).replace(",", ""))
        rows.append({"package": package, "steps": steps, "arguments": proof})
    executable = ver / "target/proving/stwo_cairo_verifier.executable.json"
    program = out / "stwo_cairo_verifier.all_builtins.executable.json"
    program.write_text(json.dumps(with_all_builtins(json.loads(executable.read_text()))))
    rows[0].update(prove(bins, program, ver / rows[0]["arguments"],
                         out / "calibration.proof.json", out / "calibration.log", max_gib))
    (out / "calibration.json").write_text(json.dumps(rows, indent=1))


# ---------------------------------------------------------------------------- report


def fit(points):
    """Least squares y = a + b * n over the points at n >= 2^FIT_MIN_LOG_N."""
    pts = [(n, y) for n, y in points if n >= 2**FIT_MIN_LOG_N]
    if len(pts) < 3:
        return None
    mn = sum(n for n, _ in pts) / len(pts)
    my = sum(y for _, y in pts) / len(pts)
    b = sum((n - mn) * (y - my) for n, y in pts) / sum((n - mn) ** 2 for n, _ in pts)
    return my - b * mn, b


def trace_cells(r):
    return r["cells"]["base"] + r["cells"]["interaction"]


def padded_rows(r, name):
    v = r["log_sizes"].get(name)
    if v is None:
        return 0
    return sum(2**x for x in v) if isinstance(v, list) else 2**v


def fmt(x, digits=0):
    return f"{x:,.{digits}f}"


def report(stwo, out):
    meta = json.loads((out / "meta.json").read_text())
    rows = [json.loads(line) for line in (out / "results.jsonl").read_text().splitlines()]
    rows = list({(r["bench"], r["n"]): r for r in rows}.values())  # last run of each point wins
    cols = component_columns(stwo)
    lines = []
    w = lines.append
    m = meta["machine"]
    w(f"Machine: {m['cpu']}, {m['cores']} cores, {m['ram_gib']:.0f} GiB, {m['os']}.")
    w(f"Toolchains: {meta['versions']['scarb']}; {meta['versions']['rustc']}; "
      f"stwo-cairo {meta['versions']['stwo_cairo']}.")
    w("")
    fits = {}
    for bench in BENCHES:
        br = sorted((r for r in rows if r["bench"] == bench), key=lambda r: r["n"])
        if not br:
            continue
        w(f"### {bench}")
        w("")
        w("| N | Steps (`scarb execute`) | Max memory address | Trace cells (base + interaction) "
          "| Prove + verify (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) "
          "| Load avg (1/5/15 min) | Status |")
        w("|---:|---:|---:|---:|---:|---:|---:|---:|---|---|")
        for r in br:
            ok = r["status"] == "verified"
            w(f"| 2^{r['log_n']} | {fmt(r['steps'])} | {fmt(r['max_memory_address'])} | "
              + (f"{fmt(trace_cells(r))} | {r['prove_s']:.2f} | {fmt(r['steps'] / r['prove_s'])} | "
                 f"{r['max_rss_b'] / 2**30:.1f} | " if ok else "— | — | — | — | ")
              + (f"{r['peak_footprint_b'] / 2**30:.1f} | " if "peak_footprint_b" in r else "— | ")
              + f"{'/'.join(f'{x:.1f}' for x in r['load_avg'])} | {r['status']} |")
        ok = [r for r in br if r["status"] == "verified"]
        f = {
            "steps": fit([(r["n"], r["steps"]) for r in br]),
            "memory": fit([(r["n"], r["max_memory_address"]) for r in br]),
            "cells": fit([(r["n"], trace_cells(r)) for r in ok]),
            "prove_s": fit([(r["n"], r["prove_s"]) for r in ok]),
            "rss_gib": fit([(r["n"], r["max_rss_b"] / 2**30) for r in ok]),
            "footprint_gib": fit([(r["n"], r["peak_footprint_b"] / 2**30) for r in ok]),
        }
        uses = {}
        for rel in ok[-1]["relation_uses"] if ok else []:
            uses[rel] = fit([(r["n"], r["relation_uses"].get(rel, 0)) for r in ok])
        rows_fit = {}
        for name in {k for r in ok for k in r["log_sizes"]}:
            rows_fit[name] = fit([(r["n"], padded_rows(r, name)) for r in ok])
        fits[bench] = dict(f, relation_uses=uses, padded_rows=rows_fit)
        w("")
        w("| Fit over N ≥ 2^14 (" + ", ".join(f"2^{r['log_n']}" for r in ok
                                              if r["log_n"] >= FIT_MIN_LOG_N) + ") | Fixed | Per op |")
        w("|---|---:|---:|")
        for key, label, unit_fixed, unit_op, scale in [
                ("steps", "Steps", "", "", 1), ("memory", "Memory addresses", "", "", 1),
                ("cells", "Trace cells, padded (base + interaction)", "", "", 1),
                ("prove_s", "Prove + verify time", " s", " µs", 1e6),
                ("rss_gib", "Peak RSS", " GiB", " KiB", 2**20),
                ("footprint_gib", "Peak footprint", " GiB", " KiB", 2**20)]:
            if f[key]:
                d = 2 if unit_fixed else 0
                w(f"| {label} | {f[key][0]:,.{d}f}{unit_fixed} | {f[key][1] * scale:,.2f}{unit_op} |")
        if len(ok) >= 2 and ok[-1]["log_n"] >= FIT_MIN_LOG_N:
            a, b = ok[-2], ok[-1]
            w(f"| Prove + verify time, marginal from 2^{a['log_n']} to 2^{b['log_n']} | | "
              f"{(b['prove_s'] - a['prove_s']) / (b['n'] - a['n']) * 1e6:,.2f} µs |")
        w("")
        if ok:
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
            padded = padded_rows(r, name) / n
            unpadded = count * per / n
            tot = [tot[0] + per, tot[1] + padded, tot[2] + unpadded * c, tot[3] + padded * c]
            w(f"| `{name}` | {cols[name]['base']} + {cols[name]['interaction']} | {per} | "
              f"{padded:g} (2^{r['log_sizes'][name]}) | {fmt(unpadded * c)} | {fmt(padded * c)} |")
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

    cal = out / "calibration.json"
    if cal.exists():
        w("### Calibration")
        w("")
        w("| Program | Steps | Prove + verify (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) "
          "| Trace cells (base + interaction) | Load avg | Status |")
        w("|---|---:|---:|---:|---:|---:|---:|---|---|")
        for c in json.loads(cal.read_text()):
            if "status" not in c:
                w(f"| `{c['package']}` | {fmt(c['steps'])} | executed only | | | | | | |")
            elif c["status"] != "verified":
                w(f"| `{c['package']}` | {fmt(c['steps'])} | — | — | — | "
                  f"{c.get('peak_footprint_b', 0) / 2**30:.1f} | — | "
                  f"{'/'.join(f'{x:.1f}' for x in c['load_avg'])} | {c['status']} |")
            else:
                w(f"| `{c['package']}` | {fmt(c['steps'])} | {c['prove_s']:.1f} | "
                  f"{fmt(c['steps'] / c['prove_s'])} | {c['max_rss_b'] / 2**30:.1f} | "
                  f"{c['peak_footprint_b'] / 2**30:.1f} | "
                  f"{fmt(trace_cells(c))} | {'/'.join(f'{x:.1f}' for x in c['load_avg'])} | verified |")
        w("")

    if all(b in fits and fits[b]["prove_s"] for b in BENCHES):
        extrapolate(fits, w)
    text = "\n".join(lines) + "\n"
    (out / "report.md").write_text(text)
    print(text)


def extrapolate(fits, w):
    w("### Extrapolation to TinyLlama (estimate)")
    w("")
    w("Op counts: BLAKE2s compressions = leaf M31s / 16 + internal hashes, which reproduces the "
      f"§3 total of {fmt(TINYLLAMA_COMPRESSIONS)}; QM31 mul-adds = leaf M31s × OODS values / "
      "committed columns, one per queried value and sample point; ingested felts = leaf M31s "
      "+ 4 × OODS values + 8 × internal hashes. Costs add linearly; each segment is one proof, "
      "so it pays once the largest of the three fixed costs.")
    w("")
    w("| Workload | Segments | BLAKE2s | QM31 mul-adds | Ingested felts | Steps | Trace cells "
      "| Prove time: BLAKE2s / QM31 / ingestion / fixed (s) | Prove time (s) "
      "| Peak footprint (GiB) |")
    w("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    head = None
    for label, t in TINYLLAMA.items():
        ops = {
            "blake2s": t["leaf_m31"] / 16 + t["hashes"],
            "qm31": t["leaf_m31"] * t["oods"] / t["columns"],
            "m31": t["leaf_m31"] + 4 * t["oods"] + 8 * t["hashes"],
        }

        def per_op(key):
            return {b: fits[b][key][1] * ops[b] for b in BENCHES}

        def fixed(key):
            return max(fits[b][key][0] for b in BENCHES)

        def total(key):
            return t["segments"] * fixed(key) + sum(per_op(key).values())
        split = per_op("prove_s")
        w(f"| {label} | {t['segments']} | {fmt(ops['blake2s'])} | {fmt(ops['qm31'])} | "
          f"{fmt(ops['m31'])} | {fmt(total('steps'))} | {fmt(total('cells'))} | "
          + " / ".join(fmt(split[b]) for b in BENCHES)
          + f" / {fmt(t['segments'] * fixed('prove_s'))} | {fmt(total('prove_s'))} | "
          + (f"{fmt(fixed('footprint_gib') + sum(per_op('footprint_gib').values()))} |"
             if t["segments"] == 1 else "see the largest segment |"))
        if t["segments"] == 1:
            head = (label, ops, total)
    w("")
    label, ops, total = head

    def combined(kind):  # per relation (uses) or component (padded rows): largest fixed + per op
        acc = {}
        for name in {n for b in BENCHES for n, f in fits[b][kind].items() if f}:
            fs = [(b, fits[b][kind][name]) for b in BENCHES if fits[b][kind].get(name)]
            acc[name] = max(f[0] for _, f in fs) + sum(f[1] * ops[b] for b, f in fs)
        return acc
    mem = total("memory")
    rel = combined("relation_uses")
    top_rel = max(rel, key=rel.get)
    comp = combined("padded_rows")
    top_comp = max(comp, key=comp.get)
    w(f"Checks for the largest segment, {label}:")
    w("")
    w("| Check | Estimate | Bound |")
    w("|---|---:|---|")
    w(f"| Memory addresses (execution + ingested arguments) | {fmt(mem)} (2^{math.log2(mem):.2f}) "
      f"| 2^{LOG_MEMORY_ADDRESS_BOUND} = {fmt(2**LOG_MEMORY_ADDRESS_BOUND)} |")
    w(f"| Largest relation total uses, `{top_rel}` | {fmt(rel[top_rel])} "
      f"({rel[top_rel] / M31_P:.2f} P) | P = {fmt(M31_P)} |")
    w(f"| Largest component, `{top_comp}` | {fmt(comp[top_comp])} padded rows, log size "
      f"{math.ceil(math.log2(comp[top_comp]))} | |")
    w(f"| Steps | {fmt(total('steps'))} (2^{math.log2(total('steps')):.2f}) | |")
    w("")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--stwo-cairo", type=Path, default=REPO / "runs/stwo-cairo")
    ap.add_argument("--out", type=Path, default=REPO / "runs/stwo-unit-costs")
    ap.add_argument("--bench", choices=BENCHES, action="append")
    ap.add_argument("--log-n", type=int, action="append", help="default: 12..18")
    ap.add_argument("--max-gib", type=float, default=35,
                    help="stop a proof whose memory footprint passes this: out of memory")
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
                    a.max_gib)
        if not a.skip_calibration:
            calibrate(stwo, bins, out, a.max_gib)
    report(stwo, out)


if __name__ == "__main__":
    main()
