# STARK-native route vs the SP1 BN254 wrap for settling SASH proofs

The question raised in review: SASH already produces Circle STARK proofs, so why wrap them in an SP1 proof, and is there a STARK-native route to Ethereum instead? This write-up records what we found on 2026-10-27, with sources. It informs M2 (SP1 guest) and M3 (wrap choice); it changes no decision by itself.

## Short answer

- **No off-the-shelf STARK-native route exists today**, for two reasons:
  - StarkWare's recursion stack doesn't accept SASH's proof format.
  - SHARP's Ethereum verifier still verifies Stone proofs.
- **A custom STARK-native route is plausible**, and it needs no re-proving:
  1. Write a verifier for SASH's current format as a Cairo program, or as a custom stwo circuit.
  2. Prove it with S-two, whose Cairo AIR has native BLAKE2s and QM31 opcodes.
  3. Submit it to SHARP as a Cairo program, through Herodotus Atlantic, which accepts Cairo PIEs only. SHARP registers a fact on L1. A custom stwo circuit gets there only through a Cairo run of `stwo_circuit_verifier`.
  
  It is transparent, with no BN254 and no trusted setup. It is also many months of work that nobody has done.
- **The SP1 route works with today's proofs**, but it is weak exactly where STARK tooling is strong: SP1 has no BLAKE2s or M31 precompile. M2's go/no-go should therefore compare it against the Cairo/S-two route.

## Why SASH proofs can't settle on Ethereum directly

The EIP facts below are verified; the sizes and gas multiples are estimates from an unpublished run (`docs/EVM-GAS-MODEL.md` §1, §3):
- **No BLAKE2s on the EVM.** The precompile at `0x09` (`EIP-152`) is only the BLAKE2b `F` compression function, over 64-bit words. SASH hashes with 32-bit BLAKE2s.
- **One segment is bigger than a block.** One segment proof (`B0L0`) is about 13.45 MB by stwo's in-memory `size_estimate()` plus its GKR bytes (its JSON bundle is 33.8 MB), above the Osaka block size limit of `EIP-7934` (about 8 MiB). Its calldata floor alone is about 28.5x the `EIP-7825` per-transaction cap.
- **All 23 segments need at least ~800x the cap** under Osaka (`docs/EVM-GAS-MODEL.md` §3). These figures are estimates from the sizes of an unpublished run.
- **Any route must recurse off-chain first.** SASH proofs have to become one small proof before they reach L1.

## SASH's proof format, at `eb95bfa`

- **Stwo fork:** a vendored NitrooZK fork of stwo v2.1.0, plus SASH's own verifier edits.
- **Merkle trees:** non-lifted, mixed-degree, hashed with M31-reduced BLAKE2s (`vcs::Blake2sM31MerkleChannel`).
- **Trees per segment:** 6 to 10, composition tree included: shape, model, base and interaction, then one per boundary side (residual stream in and out, K/V), then the composition tree (`docs/EVM-GAS-MODEL.md` §1).
- **GKR:** one LogUp-GKR batch proof per segment, covering the lookups of up to five component families (attention key tiles, dequant, residual stream, Freivalds, attnout; `head` carries one too). It is verified outside stwo's `verify`, between trees 2 and 3, by `partially_verify_batch` from stwo's `prover` module, which needs the `prover` feature and a nightly toolchain. This repo's `sp1/patches/0002-move-gkr-verifier-to-core-lookups.patch` moves `partially_verify_batch` and the sum-check verifier to `stwo::core::lookups`, so the verifier build needs neither.
- **PCS:** `log_blowup = 1`, 100 queries, 0 PoW bits, for 100 conjectured bits.

## StarkWare's stack today

- **Repository:** development moved to `starkware-libs/proving` (stwo 2.4) in July 2026.
- **Recursion verifier:** `circuits-stark-verifier` takes any constraint system through its `Statement` trait. Its proof format is fixed:
  - exactly 4 trees, all lifted to one height;
  - lifted Blake2s Merkle trees;
  - a newer transcript (salt, FRI-config mixing, PoW);
  - LogUp only, no GKR.
  
  SASH proofs don't match, and roots of different tree layouts can't be converted.
- **Cairo verifier on `main`:** checks only proofs of circuits built with StarkWare's circuit library (its gate DSL). The Cairo verifiers for Cairo CPU-AIR proofs are frozen on the `cairo-verifier-frozen` branch.
- **The L1 path:** Ethereum verifies a Stone proof of SHARP's bootloader, at about 6.2M gas across ~12 transactions per batch, shared by every task in it. S-two proves the tasks inside that recursion. StarkWare announced in March 2026 that the L1 verifier will move to S-two circuit proofs. That hasn't shipped and has no date.
- **Third-party access:**
  - Herodotus Atlantic takes Cairo PIEs (the program, memory and builtin data of a Cairo run), proves them with Stone or, if `sharpProver` selects it, S-two, and registers SHARP facts on L1. It is a paid service: credits bought under an API key, or pay-per-job via x402 from a wallet. There is no public price list.
  - Herodotus Integrity (on Starknet) handles Stone proofs only.

## Routes

| Route | Exists today | Works with today's SASH proofs | L1 cost | Trust | Verdict |
|---|---|---|---|---|---|
| **SP1 wrap** (current plan): SP1 runs SASH's verifier per segment, aggregates, BN254 Groth16 or PLONK | Verifier contracts yes; guest and aggregator no | Yes | ~270k (Groth16) to ~300k (PLONK) verify; settlement ~474k with Groth16, ~504k with PLONK (estimates, `docs/EVM-GAS-MODEL.md` §5) | BN254, setup ceremony, SP1 circuits | Current M3 plan; proving cost unmeasured |
| **Raw Solidity verifier** | No | No: block and tx limits | Billions of gas | Hash-based | Dead |
| **Cairo/circuit verifier for today's format**, S-two proof, SHARP fact | SHARP, S-two and Atlantic yes; the SASH verifier no; SHARP acceptance of a task this size untested | Yes, no re-proving | Our settlement + a fact lookup; SHARP root shared | Transparent; SHARP verifier (2/4 multisig, 8-day delay) and its unreproduced allowlist of recursive verifier programs | Most credible STARK-native route; many months |
| **Port SASH to stwo 2.4** (lifted trees, 4 trees, no GKR), StarkWare recursion, SHARP or its future L1 circuit verifier | Off-chain half yes; L1 circuit verifier no | No: re-prove TinyLlama | Unknown | Transparent | Strategic end state; depends on StarkWare and SASH upstream |
| **Own Solidity verifier** for one small recursed Keccak-channel proof | No | After recursion | Unmeasured: several million to ~20M gas (UPP's unverified claim), so possibly split across txs; unshared | Hash-based, our own unaudited code | Only if dropping BN254 is a hard requirement |
| **Starknet (Integrity) then L1** | Stone only | No | Amortised | Adds Starknet Stage 1 trust | Dominated by the SHARP route |
| **Other endings** (RISC Zero, Airbender: BN254 wraps; WHIR: hash-based research) | RISC Zero and Airbender wrap in BN254 (Groth16, FFLONK); WHIR is research | No hash-only SNARK verifier yet | WHIR PCS alone ~5.6–6.4M in software, ~4.3M with experimental precompiles | — | Watch WHIR |

## SP1's weak spots

- **No BLAKE2s or M31/QM31 precompile in SP1 v6.10.** The ~4.64M BLAKE2s compressions and the field arithmetic run as plain RISC-V. Our earlier estimate was roughly 10^10 cycles per inference. `docs/EVM-GAS-MODEL.md` §6 measures the unit costs, at 2,139 cycles per compression and 148 per DEEP-quotient step. With a `bincode` proof, its extrapolation for all 23 segments comes to ~2.3e10 cycles from the measured parts alone. Estimating the unmeasured work (OODS evaluation, FRI, the channel and GKR) from #41's synthetic runs raises it to ~3.75–4.92e10. #45 measures it. S-two Cairo and StarkWare's circuit library make exactly these operations native.
- **Large guest input.** `verify_composed` reads serde JSON: about 761 MB per run, 84.6 MB for the `head` segment alone. Decoding JSON in the guest costs ~30 cycles per byte, so the proof buffer moves to `bincode`, at 8x fewer cycles (`docs/EVM-GAS-MODEL.md` §6.2, `SPEC.md` §1.1).
- **No verifier-only build.** The verifier sits inside SASH's monolithic prover crate, which always enables stwo's `prover` and `parallel` features, rayon and the CUDA modules, and needs nightly. A guest means splitting out and maintaining a verifier crate. Prior art shows a stwo verifier compiling inside SP1 (`Okm165/stwo-verifier-sp1`), but not this fork or GKR.
- **Weak Groth16 setup.** L2BEAT rates SP1 v6.1.0's Groth16 phase-2 setup red: Succinct ran it, with 18 participants and no public call. The PLONK option rests on Aztec Ignition and is rated green. If we stay on SP1, PLONK costs ~30k more gas for a stronger setup.

## S-two unit costs (M2)

M2's comparison with S-two is a bounded microbenchmark: what S-two's Cairo AIR charges for the three operations that dominate verifying a SASH segment. No Cairo SASH verifier is built. [Beside #38's SP1 unit costs](#beside-38s-sp1-unit-costs) puts #38's SP1 figures next to these; #48 makes the comparison and the go/no-go.

### Setup

- **Machines.** Each machine ran the whole series and the calibration, so every fit uses points from one machine.
  - **Apple M3 Max:** 14 cores (10 performance + 4 efficiency), 36 GiB, macOS 15.7.3. #38's SP1 runs share it; the runs recorded here did not overlap with them. Each proof waits up to 3 minutes for the 1-minute load average to fall below 3, and records the load averages next to its timing.
  - **AMD EPYC 7R13** (`/proc/cpuinfo`), on Hugging Face Jobs' `cpu-performance` flavor, which the Jobs API lists as 32 vCPU, 256 GB, $0.031667 per minute. The job saw:
    - CPU: 32 logical CPUs (16 cores × 2 threads), `nproc` 32, cgroup CPU quota 32;
    - memory: 247 GiB `MemTotal`, cgroup memory limit 275 GB;
    - OS: Ubuntu 24.04.5 LTS (`ubuntu:24.04` image), Linux 6.12.94 (amzn2023), with AVX2 and SHA-NI.
    
    Job `6ae1430d41e90535df76ce4c` ran for 626 s, build included, about $0.33. The proofs ran back to back (`--settle-s 0`), so its load averages are mostly the previous proof's threads winding down.
  - **The conclusions use the EPYC figures.** Only the EPYC proved BLAKE2s up to 2^18: its BLAKE2s fit rests on five points with no swap, against three on the M3 Max, the last of which swapped. A 32-vCPU, 256 GB server is also closer to the host a prover would run on. The M3 Max figures stay as a second machine, showing how much the times depend on the hardware.
- **Toolchain, the same on both machines:** `starkware-libs/stwo-cairo@2b1fb4ded5496fb2db4e8261cd5ee5ef6b710839`, with its `rust-toolchain.toml` (`nightly-2026-01-15`, rustc 1.94.0-nightly `86a49fd71`) and `.tool-versions` (`scarb 2.18.0`, `e6144df0f`).
- **Prover:** `run_and_prove` from `crates/dev_utils`, built on each machine with `RUSTFLAGS="-C target-cpu=native"` and run with `--program_type executable --verify`. It uses the default production parameters (`crates/prover/src/prover.rs:409-424`): Blake2s channel, `log_blowup 1`, 70 queries, `pow_bits 26`, 96 bits. Every proof in the tables passed `--verify`.
- **Benchmarks:** the Scarb executable `bench/stwo-cairo/` (`stwo_unit_costs`), compiled like stwo-cairo's `proving` profile (`enable-gas = false`, `unsafe-panic = true`). Its arguments select the benchmark and N:
  - `blake2s`: N chained `core::blake::blake2s_compress` calls, `byte_count += 64` each;
  - `qm31`: N chained fused mul-adds `acc * b + c` on `core::qm31::qm31`, as `verifier_core/src/fields/qm31/opcode.cairo:50` does;
  - `m31`: N M31 values read from the arguments as `Array<core::qm31::m31>`: the range-checked deserialization only, with no work per value after it.
- **One workaround:** at this commit, `run_and_prove --program_type executable` fails on any Scarb executable. The executable passes only the builtins it uses, but the prover reads all 11 public-segment pointers (the adapter's bootloader context), so it panics or reports "Constraints not satisfied". The script prepends a Standalone header that receives all 11 builtins and calls the executable's Bootloader entrypoint. That adds 15 steps (16 for the verifier, which uses `bitwise`). The tables give `scarb execute`'s step counts, without them.
- **Reproduce:** `python3 -I bench/stwo-cairo/run.py` clones the pin into `runs/stwo-cairo/`, writes proofs and logs to `runs/stwo-unit-costs/`, and prints every table below for the machine it runs on. It asserts the two calibration step counts, and that the rows it derives from `get_execution_resources` give every growing component's log size in each proof's claim. On macOS it times with `/usr/bin/time -l`, on Linux with GNU `/usr/bin/time -v`. It takes about an hour on the M3 Max, mostly waiting for the load to settle, and 10 minutes on the EPYC with `--settle-s 0`. Neither `make check` nor CI runs it.

### Per-N measurements: Apple M3 Max

Steps and max memory address come from `scarb execute --print-resource-usage`. Time, peak RSS and peak footprint come from `/usr/bin/time -l` over the whole `run_and_prove` command: the Cairo VM run, adaptation, proving, verification and writing the proof. The VM run and adaptation take about 3 s of the calibration's 48.6 s. Trace cells are the prover's "Witness trace cells" (base + interaction trees); the 543,100,528-cell preprocessed tree is the same in every run. Cells/step is that whole-run total over the steps. Peak footprint includes memory that macOS compressed or swapped out, so it is the figure that has to fit in 36 GiB.

**BLAKE2s compressions:**

| N | Steps | Max memory address | Trace cells | Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 57,414 | 53,633 | 164,617,856 | 2,867 | 5.74 | 10,002 | 15.3 | 12.2 | 3.0/3.6/3.5 | verified |
| 2^13 | 114,758 | 106,881 | 242,163,328 | 2,110 | 6.20 | 18,509 | 15.7 | 12.2 | 2.8/3.5/3.5 | verified |
| 2^14 | 229,446 | 213,377 | 397,254,272 | 1,731 | 8.41 | 27,283 | 18.0 | 13.8 | 2.7/3.5/3.4 | verified |
| 2^15 | 458,822 | 426,369 | 707,436,160 | 1,542 | 13.19 | 34,786 | 19.8 | 18.3 | 2.9/3.5/3.5 | verified |
| 2^16 | 917,574 | 852,353 | 1,327,799,936 | 1,447 | 25.57 | 35,885 | 20.3 | 26.9 | 3.0/3.6/3.5 | verified |
| 2^17 | 1,835,078 | 1,704,321 | — | — | — | — | — | 37.0 when stopped | 3.0/3.7/3.6 | out of memory |
| 2^18 | | | | | | | | | | not attempted |

**Out of memory at N = 2^17:** the script stops a proof whose footprint passes 35 GiB. In an earlier pass without that limit, macOS killed the 2^17 proof at a peak footprint of 40.7 GiB, after its RSS reached 22.9 GiB. 2^18 needs more memory still and was not attempted. Both were proved on the EPYC ([below](#per-n-measurements-and-fits-amd-epyc-7r13)), at 46.0 and 79.5 GiB of peak RSS. The M3 Max's BLAKE2s fit uses its three points from 2^14 to 2^16.

**QM31 fused mul-adds:**

| N | Steps | Max memory address | Trace cells | Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 32,851 | 29,082 | 40,859,536 | 1,244 | 5.28 | 6,222 | 14.6 | 11.6 | 2.9/3.3/3.3 | verified |
| 2^13 | 65,619 | 57,754 | 44,971,920 | 685 | 4.83 | 13,586 | 14.7 | 11.4 | 2.7/3.2/3.3 | verified |
| 2^14 | 131,155 | 115,098 | 53,196,688 | 406 | 4.99 | 26,284 | 14.7 | 11.5 | 2.6/3.2/3.3 | verified |
| 2^15 | 262,227 | 229,786 | 69,646,224 | 266 | 5.09 | 51,518 | 14.7 | 11.6 | 2.8/3.2/3.3 | verified |
| 2^16 | 524,371 | 459,162 | 102,545,296 | 196 | 5.77 | 90,879 | 14.9 | 11.6 | 3.0/3.3/3.3 | verified |
| 2^17 | 1,048,659 | 917,914 | 168,343,440 | 161 | 6.24 | 168,054 | 15.2 | 12.2 | 3.0/3.4/3.4 | verified |
| 2^18 | 2,097,235 | 1,835,418 | 299,939,728 | 143 | 7.90 | 265,473 | 16.5 | 12.8 | 2.9/3.4/3.4 | verified |

**M31 ingestion:**

| N | Steps | Max memory address | Trace cells | Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 69,693 | 57,719 | 44,257,152 | 635 | 5.46 | 12,764 | 14.7 | 11.4 | 1.7/2.0/3.0 | verified |
| 2^13 | 139,325 | 115,063 | 51,773,312 | 372 | 5.17 | 26,949 | 14.5 | 11.6 | 2.7/2.2/3.1 | verified |
| 2^14 | 278,589 | 229,751 | 66,805,632 | 240 | 5.15 | 54,095 | 14.7 | 11.6 | 2.7/2.3/3.1 | verified |
| 2^15 | 557,117 | 459,127 | 96,870,272 | 174 | 5.35 | 104,134 | 14.9 | 11.7 | 3.0/2.5/3.1 | verified |
| 2^16 | 1,114,173 | 917,879 | 156,999,552 | 141 | 6.48 | 171,940 | 15.4 | 12.1 | 3.0/2.7/3.1 | verified |
| 2^17 | 2,228,285 | 1,835,383 | 277,258,112 | 124 | 7.92 | 281,349 | 16.3 | 12.8 | 2.9/2.8/3.1 | verified |
| 2^18 | 4,456,509 | 3,670,391 | 517,775,232 | 116 | 12.32 | 361,730 | 19.0 | 15.0 | 3.0/3.0/3.2 | verified |

### Fits: Apple M3 Max

Fixed cost plus per-op cost, by least squares over the points at N ≥ 2^14: 2^14 to 2^16 for BLAKE2s, 2^14 to 2^18 for the others.

| Per op | BLAKE2s compression | QM31 mul-add | M31 ingested |
|---|---:|---:|---:|
| Steps | 14 | 8 | 17 |
| Memory addresses | 13 | 7 | 14 |
| Trace cells, padded at N = 2^k (base + interaction) | 18,932 | 1,004 | 1,835 |
| Trace cells per step, marginal | 1,352 | 126 | 108 |
| Time, fit | 353.2 µs | 11.7 µs | 29.3 µs |
| Time, marginal between the two largest N | 377.8 µs | 12.7 µs | 33.6 µs |
| Time fit residuals, N = 2^14, 2^15, … (s) | +0.40, −0.60, +0.20 | −0.01, −0.10, +0.20, −0.10, +0.02 | +0.20, −0.08, +0.09, −0.39, +0.17 |
| Peak footprint | 277.5 KiB | 5.9 KiB | 14.7 KiB |
| **Fixed** (time; peak footprint) | 2.2 s; 9.6 GiB | 4.8 s; 11.4 GiB | 4.5 s; 11.2 GiB |

- **Fixed costs dominate.** Every run carries the preprocessed tree and the fixed-size lookup tables: 36,735,680 base + interaction cells (87,067,408 with BLAKE2s's XOR tables), 4–6 s and 10–11 GiB of footprint.
- **Uncertainty.**
  - The QM31 and M31 timings stay nearly flat up to 2^16, so their per-op times rest mainly on 2^17 and 2^18. The marginal slopes between those two points are 8% and 15% above the fits.
  - The BLAKE2s curve is convex: its fit intercept (2.2 s) is below the measured 2^12 time (5.7 s), the residuals swing by ±0.6 s over three points, and the marginal slope rises from 292 µs (2^14 to 2^15) to 378 µs (2^15 to 2^16). The 2^16 proof pushed about 3.6 GiB into swap, which may inflate it.
  - Two earlier BLAKE2s passes with an earlier build fitted 318 µs (at a 1-minute load of 11–13) and 357 µs per compression. The range is about 320–400 µs.
- **Opcode mix per op:**
  - BLAKE2s: 1 `blake_compress_opcode` plus 13 ordinary instructions (loop, `byte_count` add with its range check).
  - QM31: 2 `qm_31_add_mul_opcode` instances, since the mul and the add are separate opcodes, plus 6 loop instructions.
  - M31: 17 ordinary instructions and 2 range-check builtin instances per value, with no special opcode. This is the deserialization alone; the benchmark does nothing with the values afterwards.

### Per-N measurements and fits: AMD EPYC 7R13

The same measurements as on the M3 Max. Time and peak RSS come from GNU `/usr/bin/time -v`. Linux reports no peak footprint, and the job used no swap, so peak RSS is the figure that has to fit in RAM.

**BLAKE2s compressions:**

| N | Steps | Max memory address | Trace cells | Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 57,414 | 53,633 | 164,617,856 | 2,867 | 8.32 | 6,901 | 13.3 | 9.4/5.9/4.8 | verified |
| 2^13 | 114,758 | 106,881 | 242,163,328 | 2,110 | 8.36 | 13,727 | 14.0 | 10.7/6.3/4.9 | verified |
| 2^14 | 229,446 | 213,377 | 397,254,272 | 1,731 | 10.24 | 22,407 | 16.5 | 14.0/7.1/5.2 | verified |
| 2^15 | 458,822 | 426,369 | 707,436,160 | 1,542 | 14.56 | 31,512 | 20.6 | 12.0/6.9/5.2 | verified |
| 2^16 | 917,574 | 852,353 | 1,327,799,936 | 1,447 | 21.27 | 43,139 | 28.8 | 12.6/7.3/5.3 | verified |
| 2^17 | 1,835,078 | 1,704,321 | 2,568,527,488 | 1,400 | 36.07 | 50,875 | 46.0 | 16.3/8.6/5.8 | verified |
| 2^18 | 3,670,086 | 3,408,257 | 5,049,982,592 | 1,376 | 63.56 | 57,742 | 79.5 | 19.0/10.4/6.6 | verified |

**QM31 fused mul-adds:**

| N | Steps | Max memory address | Trace cells | Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 32,851 | 29,082 | 40,859,536 | 1,244 | 7.46 | 4,404 | 12.2 | 14.9/10.8/6.9 | verified |
| 2^13 | 65,619 | 57,754 | 44,971,920 | 685 | 7.51 | 8,738 | 12.3 | 17.5/11.4/7.2 | verified |
| 2^14 | 131,155 | 115,098 | 53,196,688 | 406 | 7.57 | 17,326 | 12.3 | 21.0/12.4/7.5 | verified |
| 2^15 | 262,227 | 229,786 | 69,646,224 | 266 | 7.78 | 33,705 | 12.5 | 19.4/12.2/7.5 | verified |
| 2^16 | 524,371 | 459,162 | 102,545,296 | 196 | 8.52 | 61,546 | 13.0 | 19.2/12.3/7.6 | verified |
| 2^17 | 1,048,659 | 917,914 | 168,343,440 | 161 | 8.97 | 116,907 | 13.7 | 19.3/12.6/7.7 | verified |
| 2^18 | 2,097,235 | 1,835,418 | 299,939,728 | 143 | 10.83 | 193,651 | 15.2 | 16.5/12.2/7.6 | verified |

**M31 ingestion:**

| N | Steps | Max memory address | Trace cells | Cells/step | Time (s) | Steps/s | Peak RSS (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 69,693 | 57,719 | 44,257,152 | 635 | 7.91 | 8,811 | 12.3 | 18.4/12.7/7.9 | verified |
| 2^13 | 139,325 | 115,063 | 51,773,312 | 372 | 8.25 | 16,888 | 12.3 | 21.8/13.7/8.2 | verified |
| 2^14 | 278,589 | 229,751 | 66,805,632 | 240 | 7.98 | 34,911 | 12.5 | 18.7/13.3/8.1 | verified |
| 2^15 | 557,117 | 459,127 | 96,870,272 | 174 | 8.84 | 63,022 | 12.9 | 16.0/12.9/8.1 | verified |
| 2^16 | 1,114,173 | 917,879 | 156,999,552 | 141 | 9.93 | 112,203 | 13.8 | 13.8/12.5/8.0 | verified |
| 2^17 | 2,228,285 | 1,835,383 | 277,258,112 | 124 | 13.37 | 166,663 | 14.8 | 12.9/12.4/8.1 | verified |
| 2^18 | 4,456,509 | 3,670,391 | 517,775,232 | 116 | 19.32 | 230,668 | 18.3 | 11.4/12.1/8.1 | verified |

Fits over N = 2^14 to 2^18 for all three:

| Per op | BLAKE2s compression | QM31 mul-add | M31 ingested |
|---|---:|---:|---:|
| Steps; memory addresses; trace cells, padded at N = 2^k | 14; 13; 18,932 | 8; 7; 1,004 | 17; 14; 1,835 |
| Time, fit | 216.0 µs | 12.9 µs | 46.3 µs |
| Time, marginal between the two largest N | 209.7 µs | 14.2 µs | 45.4 µs |
| Time fit residuals, N = 2^14, 2^15, … (s) | −0.50, +0.28, −0.08, +0.56, −0.26 | −0.06, −0.06, +0.25, −0.14, +0.02 | +0.04, +0.14, −0.29, +0.12, −0.01 |
| Peak RSS | 269.3 KiB | 12.2 KiB | 24.5 KiB |
| **Fixed** (time; peak RSS) | 7.2 s; 12.2 GiB | 7.4 s; 12.2 GiB | 7.2 s; 12.1 GiB |
| M3 Max time fit, for comparison | 353.2 µs | 11.7 µs | 29.3 µs |

- **Only time and memory depend on the machine.** Steps, memory addresses and trace cells are identical on both machines at every N both ran, and so are the per-op step, address and cell fits.
- **BLAKE2s is linear here, at 216 µs per compression,** 39% below the M3 Max fit. Five points fit within ±0.56 s, and the marginal slope between 2^17 and 2^18 (209.7 µs) is 3% below the fit. The M3 Max's convex curve came with swap at 2^16, so it was probably memory pressure, not the prover.
- **The EPYC proves faster but runs the serial stages slower.** The log timestamps split each run into stages:
  - **Proving:** the EPYC is faster. It takes 19.6 s against 25.1 s for BLAKE2s at 2^16, and 22.6 s against 41.2 s for the calibration.
  - **Before proving:** the EPYC is slower. `run_and_prove` first loads the program and its JSON arguments, then runs the Cairo VM and adapts the trace. These stages take 21.1 s of the calibration's 45.8 s on the EPYC, against 6.1 s of 48.6 s on the M3 Max.
- **This makes M31 ingestion 58% slower: 46.3 µs per felt against 29.3 µs.**
  - **Loading the arguments:** at 2^18, `run_and_prove` spends 5.65 s on the JSON argument file before the VM starts (2.79 s at 2^17), against 1.33 s (0.68 s) on the M3 Max. That is about 22 µs per felt against 5 µs, half the EPYC's slope.
  - **The VM run and adaptation** take 2.1 s against 0.8 s at 2^18.
  - **Proving** is about the same: 10.7 s against 10.1 s.
- **Fixed costs are higher:** 7.2–7.4 s per proof, against 2.2–4.8 s on the M3 Max, and 12.1–12.2 GiB of peak RSS.

### Rows and cells per op

Per BLAKE2s compression, at N = 2^16 with 65,536 `blake_compress_opcode` instances from `get_execution_resources`:

| Component | Columns (base + interaction) | Rows, inferred | Rows, padded (log size) | Cells, unpadded | Cells, padded |
|---|---:|---:|---:|---:|---:|
| `blake_compress_opcode` | 174 + 148 | 1 | 1 (2^16) | 322 | 322 |
| `blake_round` | 212 + 120 | 10 | 16 (2^20) | 3,320 | 5,312 |
| `blake_g` | 53 + 36 | 80 | 128 (2^23) | 7,120 | 11,392 |
| `triple_xor_32` | 21 + 20 | 8 | 8 (2^19) | 328 | 328 |
| **Total** | | **99** | **153** | **11,090** | **17,354** |

- **The inferred 99 rows and 11,090 cells hold.** The proof confirms the four log sizes, and the opcode count equals N. The EPYC's 2^18 proof gives the same 153 padded rows per compression, at log sizes 18, 22, 25 and 21.
- **Padding:** at N = 2^k, padding to powers of two makes it 153 rows and 17,354 cells.
- **Whole trace:** with the opcodes, memory and lookup components a compression also touches, it is 18,932 cells per compression (the fit above).
- **QM31 mul-add:** 2 `qm_31_add_mul_opcode` rows of 73 + 24 columns, 194 cells, unpadded and padded alike (2^19 rows at N = 2^18). With everything else it is 1,004 cells.
- **What the extrapolation uses.** Padding depends on the final size, so the extrapolation doesn't scale the padded fits. It fits each growing component's unpadded rows: the opcode and builtin instance counts and memory table sizes from `get_execution_resources`, times the rows per instance above (16 addresses per `memory_address_to_id` row). It then applies the memory tables' limits: at most 2^24 small values, the rest becoming big values, in tables of at most 2^25 rows. Finally it pads each component to its power of two. The script checks this derivation on every proof: it reproduces each growing component's log size in the claim, and the remaining fixed-size components come to the same cell count in every run.

### Calibration against StarkWare's reference verifiers

The two CI commands, run from `stwo_cairo_verifier/` with `--profile proving`, execute in exactly the CI's step counts on both machines; the script asserts both. The `stwo_cairo_verifier` run was then proved:

| Program | Machine | Steps | Max memory address | Time (s) | Steps/s | Trace cells | Cells/step | Peak RSS (GiB) | Peak footprint (GiB) | Load avg | Status |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| `stwo_cairo_verifier` (`qm31_opcode`) | M3 Max | 18,152,624 | 18,556,598 | 48.6 | 373,204 | 1,939,445,536 | 107 | 19.2 | 32.5 | 3.0/4.0/5.2 | verified |
| `stwo_cairo_verifier` (`qm31_opcode`) | EPYC | 18,152,624 | 18,556,598 | 45.8 | 396,173 | 1,939,445,536 | 107 | 35.4 | — | 5.1/10.1/7.8 | verified |
| `stwo_circuit_verifier` (`qm31_opcode`) | both | 1,982,220 | 1,953,362 | executed only | | | | | | | |

- **It agrees with StarkWare's "about 1 minute".** `run_and_prove` on the Cairo verifier takes 48.6 s on the M3 Max and 45.8 s on the EPYC, VM run, adaptation and verification included. The EPYC proves in 22.6 s, against 41.2 s, but spends 21.1 s before proving, against 6.1 s.
- **It only just fits the M3 Max.** The peak footprint is 32.5 GiB, and swap grew from 22.5 to 33.2 GiB during the run, so that time may be inflated. On the EPYC it peaks at 35.4 GiB of RSS, with no swap.
- **Steps/s depends on the opcode mix.** The verifier runs at 373k steps/s on the M3 Max (396k on the EPYC), with 107 trace cells per step over the whole run. At their largest N, the microbenchmarks' whole runs reach, on the M3 Max (EPYC):
  - BLAKE2s: 36k steps/s at 2^16, 1,447 cells per step (58k at 2^18, 1,376 cells per step);
  - QM31: 265k (194k), 143 cells per step;
  - M31 ingestion: 362k (231k), 116 cells per step.

**The three ops are about half of a real verifier.** Applying the unit costs to the verifier's own op counts (22,925 `blake_compress_opcode`; 610,391 `qm_31_add_mul_opcode`, i.e. 305,196 mul-adds; 270,641 argument felts) predicts:

| | Measured | Modelled from the three ops | Measured / modelled |
|---|---:|---:|---:|
| Steps | 18,152,624 | 7,363,494 | 2.47 |
| Max memory address | 18,556,598 | 6,223,777 | 2.98 |
| `MemoryAddressToId` uses | 71,827,552 | 39,277,054 | 1.83 |
| Trace cells | 1,939,445,536 | 1,324,131,001 | 1.46 |
| Time (s), EPYC | 45.8 | 28.9 | 1.59 |
| Time (s), M3 Max | 48.6 | 24.4 | 1.99 |

Everything but time is the same on both machines. The rest is AIR evaluation, channel draws, FRI folding, Merkle path walking and control flow. The extrapolation below gives both the modelled figures and the figures scaled by these ratios. The verifier checks a Cairo AIR proof, not a SASH one, so the ratios are an indication, not a bound.

### Extrapolation to TinyLlama (estimate)

From `docs/EVM-GAS-MODEL.md` §1 (65,967,346 leaf M31s, 519,784 OODS values, 501,197 columns, 518,489 internal hashes) and §3 (4,641,448 BLAKE2s compressions). Assumed multipliers:
- **BLAKE2s compressions** = leaf M31s / 16 + internal hashes. This reproduces §3's 4,641,448 exactly, and gives 525,665 for `head`.
- **QM31 mul-adds** = 100 query positions × (OODS values + two-sample columns), with two-sample columns = OODS values − columns: one per query position and (column, sample) pair, as `accumulate_row_quotients` (`verifier_core/src/pcs/quotients.cairo:263`) does. SASH proves with 100 queries. A periodicity sample is added to every column sampled at two points (`build_samples_with_randomness_and_periodicity`, which the pinned Cairo verifier mirrors at `quotients.cairo:79-100`), and no column has more than two. This is the count #38's SP1 model uses: 53,837,100 for all 23 segments, 7,063,600 for `head`.
- **Ingested felts** = leaf M31s + 4 × OODS values + 8 × internal hashes. Each costs one M31 ingestion, including the 32-bit hash words.
- **Costs add linearly.** Each segment is its own proof, so it pays once the largest of the three fixed costs.

Steps and trace cells don't depend on the machine; the times use the EPYC's fits, with the M3 Max's beside them:

| Workload | BLAKE2s | QM31 mul-adds | Ingested felts | Steps | Trace cells | Time, fit: BLAKE2s / QM31 / ingestion / fixed (s), EPYC | Time, modelled ops (s), EPYC | Time, scaled by the calibration (s), EPYC | M3 Max: modelled ops; scaled (s) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Per inference (23 proofs) | 4,641,448 | 53,837,100 | 72,194,394 | 1,722,983,681 | 1.99 × 10^11 unpadded | 1,003 / 696 / 3,344 / 171 | 5,185–5,213 (~87 min) | 8,279 (~138 min) | 4,499–4,969; 8,960 |
| `head`, the largest segment | 525,665 | 7,063,600 | 8,533,469 | 208,937,170 | 3.31 × 10^10 padded (2.30 × 10^10 unpadded) | 114 / 91 / 395 / 7 | 605–608 | 965 | 524–579; 1,043 |

- **The modelled-ops time is the fit and the marginal slopes between the two largest N.** On the EPYC they agree within 1%. The fits imply 331k steps/s per inference and 344k for `head`, below the calibration's 396k.
- **Time per trace cell doesn't carry over between op mixes.** Multiplying the cells by the 12.6 ns per padded cell of the EPYC's largest proof gives 2,504 s per inference and 417 s for `head`. That proof is BLAKE2s at 2^18, and per marginal cell ingestion costs 25 ns on the EPYC, against 11–13 ns for BLAKE2s and QM31. On the M3 Max the largest proof was the calibration, at 25.1 ns per cell, which gives 4,990 s per inference and 830 s for `head`, above its fit and marginal times. The per-inference cells are unpadded because the model doesn't have each segment's op counts.
- **The scaled column** multiplies the fit-based time by the calibration's measured / modelled time: 1.59 on the EPYC, 1.99 on the M3 Max. It stands in for the work the three ops leave out.
- **Ingestion dominates,** at 64% of the EPYC's fit-based time per inference, against 19% for BLAKE2s, 13% for QM31 and 3% for the fixed costs. On the M3 Max the split is 47%, 36% and 14%. About half of the EPYC's ingestion cost is `run_and_prove` reading the JSON argument file, so a prover that loads its arguments faster would cut it.

Checks for `head`:

| Check | Modelled ops only | Scaled by the calibration | Bound |
|---|---:|---:|---|
| Cairo memory, execution plus ingested arguments | 175,747,825 addresses (2^27.39) | 524,003,588 (2^28.97) | `LOG_MEMORY_ADDRESS_BOUND = 29` (`crates/common/src/memory.rs:2`): 536,870,912 |
| Largest relation total uses, `MemoryAddressToId` | 1,086,725,042 (0.51 P) | 1,987,338,445 (0.93 P) | below P (`crates/cairo-air/src/verifier.rs:63-66`) |
| Steps | 208,937,170 (2^27.64) | 515,075,845 (2^28.94) | |
| Largest component | log size 26: `add_opcode_small` (51.3M rows), `blake_g` (42.1M), `assert_eq_opcode_double_deref` (34.7M), `jnz_opcode_taken` (33.7M) | | |
| Memory tables | `memory_id_to_small` full at 2^24 values; 76.8M big values in three `memory_id_to_big` tables (2^25, 2^25 and 9.6M rows) | | |
| Peak memory, modelled ops only | ~429 GiB RSS (EPYC fit); ~310 GiB footprint (M3 Max fit) | | 247 GiB on the EPYC, 36 GiB on the M3 Max |

- **What the estimate covers.**
  - The modelled column counts hashing, quotient accumulation and ingestion only. It excludes AIR evaluation, the out-of-domain evaluation of SASH's 20k–68k-column constraints that a real SASH verifier adds, and the mixed-degree Merkle tree walk. It also excludes the GKR verification, channel draws and FRI folding.
  - It assumes per-op costs stay linear from the measured sizes (at most 2^25 rows, `blake_g` in the EPYC's BLAKE2s 2^18 proof) up to 2^26 rows. FFT and commitment costs grow as n log n.
- **The modelled ops pass both checks** with a factor of 3.1 to spare on memory addresses and 2.0 on relation uses.
- **Scaled by the calibration, `head` has no margin left:** 0.98 of the memory-address bound and 0.93 P. A SASH verifier spends more on AIR evaluation than the Cairo verifier does, so a single `head` proof would probably cross a bound. Its verification would likely need splitting across several proofs, each paying the fixed cost again.
- **`head` fits neither machine.** Its modelled ops alone come to ~429 GiB of peak RSS on the EPYC fit, 1.7x the 247 GiB it has, and ~310 GiB of footprint on the M3 Max fit, about 9x its RAM. Both fits come from proofs of at most 80 GiB (EPYC) and 27 GiB (M3 Max). The per-inference time is what the EPYC's cores would take with enough memory: an estimate, not a run.
- **A 256 GB machine doesn't change the conclusion that `head` likely needs splitting.** The bounds that force the split don't depend on the machine: memory addresses and relation uses come out the same on both. And the memory to prove `head` in one piece is above 256 GB even for the modelled ops alone.
- **Trace-length bound, unchecked context.** arXiv 2606.04311 (https://arxiv.org/pdf/2606.04311) reports a 2^29 + 1 trace-length bound for the S-two AIR. The `head` estimate, 2^27.64 steps (2^28.94 scaled) with components at 2^26, would sit below it.

### Security parameters differ

stwo-cairo's default proves at 96 bits with 70 queries and `pow_bits 26`. SASH's segment proofs use 100 queries and 0 PoW bits, for 100 bits (`prover/src/util.rs:14-16`). Matching SASH's level in the recursion proof would change the query count or the PoW. The default code comment says the query count barely affects proving time.

### Beside #38's SP1 unit costs

#38's figures are in `docs/EVM-GAS-MODEL.md` §6. The two routes count in different units: RISC-V cycles and PGU in SP1's executor, against Cairo steps, trace cells and `run_and_prove` seconds here. The docs have no measured SP1 proving rate, so cycles are not converted into seconds. §6.3's GPU-hours rest on an unverified 2M PGU/s per GPU. #48 compares the routes and records the go/no-go.

| Per op | SP1 v6.10 (§6.1, §6.2): cycles; PGU | S-two Cairo (fits above): steps; padded cells; time |
|---|---|---|
| BLAKE2s compression | 2,139; 2,117 bare. Inside `hash_node`: 2,242 cycles per block, plus 12 per M31 `update` | 14; 18,932; 216 µs (EPYC), 353 µs (M3 Max) |
| QM31 fused mul-add | 318; 280 (`qm31_mul_add`) | 8; 1,004; 12.9 µs (EPYC), 11.7 µs (M3 Max) |
| DEEP-quotient step | 148; 127 per (column, sample) step of `accumulate_row_quotients` | Not measured: the extrapolation charges one QM31 mul-add per step |
| Proof input | Decoding the proof buffer: 9.58; 9.47 per byte with `bincode` (29.63; 25.37 with JSON) | Ingesting one M31 felt from the arguments: 17; 1,835; 46.3 µs (EPYC), 29.3 µs (M3 Max) |

- **Machines.** SP1's cycles and PGU come from its executor on the M3 Max and don't depend on the machine. S-two's steps and cells don't either; its times are the EPYC's and the M3 Max's.
- **Status.** Both routes measured their unit costs. Both per-inference figures below are extrapolations: estimates, not runs.

| Per inference | SP1 (§6.3), `bincode` proof | S-two (extrapolation above) |
|---|---|---|
| What the measured parts cover | BLAKE2s leaves and internal nodes, DEEP quotients, decoding | BLAKE2s compressions, QM31 mul-adds, ingestion, fixed cost per proof |
| All 23 segments, measured parts | 2.32e10 cycles (2.17e10 PGU) | 1.72e9 steps, 1.99e11 unpadded cells; 5,185–5,213 s (EPYC), 4,499–4,969 s (M3 Max) |
| All 23 segments, with the unmeasured work estimated | 3.75–4.92e10 cycles (3.45–4.51e10 PGU), from #41's synthetic SASH runs | 8,279 s (EPYC), 8,960 s (M3 Max), scaled by the stwo-cairo verifier calibration, which checks a Cairo AIR proof, not a SASH one |
| `head`, measured parts | 2.70e9 cycles (2.52e9 PGU) | 2.09e8 steps, 3.31e10 padded cells; 605–608 s (EPYC) |
| `head`, with the unmeasured work estimated | 3.32–5.73e9 cycles (3.07–5.24e9 PGU) | 965 s (EPYC); no margin left on the memory-address bound and ~429 GiB of peak RSS, so it likely needs splitting |

Neither covers OODS constraint evaluation, FRI, the channel or GKR except through its scaling. Both use the same 53,837,100 DEEP-quotient steps (QM31 mul-adds here). For BLAKE2s, S-two takes §3's 4,641,448 compressions, while SP1's model adds the per-node costs and partial blocks.

## Implications for the roadmap

- **M2:** measure SP1 cycles and proving cost on one segment early. Compare them with S-two's unit costs for BLAKE2s, QM31 mul-adds and M31 ingestion ([S-two unit costs](#s-two-unit-costs-m2)) before committing to M3. M2 builds no prototype SASH verifier in Cairo or S-two.
- **M3:** prefer PLONK over Groth16 for the setup, unless the gas difference matters.
- **SHARP route:**
  - bind to SHARP's own contracts, not to Atlantic's upgradeable Satellite diamond proxy, whose `isCairoFactValid(fact_hash, is_mocked)` also serves facts registered without on-chain verification. Two choices:
    - the SHARP call proxy `0x47312450B3Ac8b5b8e247a6bB6d523e7605bDb60`, which follows SHARP upgrades (2/4 SHARP Multisig, 8-day delay);
    - the immutable GPS statement verifier behind it, `0x4956bda1d23F75B988644329c5B06BD1494a72b6`, which pins one verifier version but stops receiving facts if SHARP retargets the proxy;
  - any Cairo verifier that hashes with Blake2s uses the Blake opcodes, which only S-two proves, so it needs `sharpProver=stwo`. That covers a verifier for SASH's current format and the default build of `stwo_circuit_verifier`. Only a custom circuit proved with a Poseidon252 channel can use the `poseidon252_verifier` build on Atlantic's default Stone prover.

## Open questions for StarkWare

1. Is a Cairo program that verifies SASH proofs in their current format, proved by S-two and submitted to SHARP (e.g. via Atlantic), a route you'd consider realistic? Are there size or cost limits per SHARP task?
2. What is the timeline for the L1 verifier of S-two circuit proofs, and will it accept third-party circuits?
3. Should SASH move to stwo 2.4 (lifted trees, LogUp without GKR) to fit the standard recursion, or be verified as it is?
4. Do you have numbers for recursively verifying one very wide stwo proof (20k–68k columns, trace log size up to 26, ~13–34 MB)?
5. Is there a versioning commitment for the stwo proof format that an L1 verifier could be built against?
6. Can a third party pin an immutable SHARP verifier version, rather than inherit its upgrades?

## Sources

- StarkWare, recursive circuit proving (March 2026): https://starkware.co/blog/minutes-to-seconds-efficiency-gains-with-recursive-circuit-proving/
- `starkware-libs/proving@3a396f51`:
  - `crates/stark_verifier/src/proof.rs` (`N_TRACES = 4`, lifting assert);
  - `crates/stark_verifier/src/statement.rs`;
  - `crates/stark_verifier/src/verify.rs`.
- SASH `sg-ai-safety-hub/zk-inference-prover@eb95bfa`:
  - `prover/src/util.rs` (PCS config);
  - `prover/src/common.rs` (channel, trees);
  - `nitroo/Cargo.toml` (stwo 2.1.0);
  - `nitroo/crates/stwo/src/core/pcs/verifier.rs`.
- L2BEAT, Starknet (SHARP 2/4 multisig, 8-day delay; Stage 1): https://l2beat.com/scaling/projects/starknet
- L2BEAT, stwo: https://l2beat.com/zk-catalog/stwo. It labels the L1 contract a "Stwo GPS statement verifier", but the verifier source is Stone CPU-AIR (https://etherscan.io/address/0x4956bda1d23F75B988644329c5B06BD1494a72b6#code), and the recursion roots stay on Stone (https://docs.starknet.io/learn/protocol/sharp).
- L2BEAT, SP1 Hypercube (Groth16 v6.1.0 phase-2 setup red, PLONK on Aztec Ignition green): https://l2beat.com/zk-catalog/sp1hypercube
- SP1 v6.10.0 syscalls (no BLAKE2s or M31): `succinctlabs/sp1@v6.10.0:crates/zkvm/entrypoint/src/syscalls/mod.rs`
- Atlantic contract addresses: https://docs.herodotus.cloud/atlantic-api/contract-addresses
- Hugging Face Jobs hardware and prices (`HfApi().list_jobs_hardware()`, 2026-10-28): `cpu-performance` is 32 vCPU, 256 GB, $0.031667 per minute. The EPYC runs are job `6ae1430d41e90535df76ce4c`.
- WHIR EVM verification: https://ethresear.ch/t/evm-verification-of-whir-over-a-31-bit-field/24902
- Herodotus Atlantic, L1 verification: https://docs.herodotus.cloud/atlantic-api/steps/l1-proof-verification
- Herodotus Integrity: https://github.com/HerodotusDev/integrity
- A stwo verifier inside SP1, prior art: https://github.com/Okm165/stwo-verifier-sp1
- EIP-7934 (block size limit): https://eips.ethereum.org/EIPS/eip-7934
- Issue #3 (M5 dropped): https://github.com/AbdelStark/verifiable-inference-ethereum/issues/3
- S-two book, BLAKE benchmarks (48-thread EPYC, 2025-07; proving time flat at 30–38 s up to 16,384 compressions): https://docs.starknet.io/learn/S-two-book/benchmarks.md
- `starkware-libs/stwo-cairo@2b1fb4ded5496fb2db4e8261cd5ee5ef6b710839`:
  - `crates/prover/src/prover.rs:409-424` (default proving parameters);
  - `crates/cairo-air/src/components/` (`blake_compress_opcode.rs`, `blake_round.rs`, `blake_g.rs`, `triple_xor_32.rs`, `qm_31_add_mul_opcode.rs`);
  - `crates/common/src/memory.rs:2` (`LOG_MEMORY_ADDRESS_BOUND`) and `crates/cairo-air/src/verifier.rs:63-66` (relation uses below P);
  - memory table limits: `crates/cairo-air/src/components/memory_address_to_id.rs:29` (16 addresses per row), `crates/adapter/src/memory.rs:66` (2^24 small values), `crates/common/src/preprocessed_columns/preprocessed_trace.rs:20` (`MAX_SEQUENCE_LOG_SIZE = 25`, the `memory_id_to_big` table size);
  - `stwo_cairo_verifier/crates/verifier_core/src/pcs/quotients.cairo` (`accumulate_row_quotients`, periodicity samples);
  - `crates/adapter/src/lib.rs` (`PublicSegmentContext::bootloader_context`, all 11 builtins);
  - `.github/workflows/cairo-ci.yaml:136-153` (reference verifier step counts).
- S-two AIR trace-length bound (2^29 + 1, unchecked): https://arxiv.org/pdf/2606.04311
