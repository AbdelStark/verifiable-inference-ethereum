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
- **GKR:** one LogUp-GKR batch proof per segment, covering the lookups of up to five component families (attention key tiles, dequant, residual stream, Freivalds, attnout; `head` carries one too). It is verified outside stwo's `verify`, between trees 2 and 3, by `partially_verify_batch` from stwo's `prover` module, which needs the `prover` feature and a nightly toolchain.
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

- **No BLAKE2s or M31/QM31 precompile in SP1 v6.10.** The ~4.64M BLAKE2s compressions and the field arithmetic run as plain RISC-V. That is roughly 10^10 cycles per inference by our estimate, unmeasured. S-two Cairo and StarkWare's circuit library make exactly these operations native.
- **Large guest input.** `verify_composed` reads serde JSON: about 761 MB per run, 84.6 MB for the `head` segment alone.
- **No verifier-only build.** The verifier sits inside SASH's monolithic prover crate, which always enables stwo's `prover` and `parallel` features, rayon and the CUDA modules, and needs nightly. A guest means splitting out and maintaining a verifier crate. Prior art shows a stwo verifier compiling inside SP1 (`Okm165/stwo-verifier-sp1`), but not this fork or GKR.
- **Weak Groth16 setup.** L2BEAT rates SP1 v6.1.0's Groth16 phase-2 setup red: Succinct ran it, with 18 participants and no public call. The PLONK option rests on Aztec Ignition and is rated green. If we stay on SP1, PLONK costs ~30k more gas for a stronger setup.

## S-two unit costs (M2)

M2's comparison with S-two is a bounded microbenchmark: what S-two's Cairo AIR charges for the three operations that dominate verifying a SASH segment. No Cairo SASH verifier is built. #48 compares these figures with #38's SP1 figures.

### Setup

- **Machine:** Apple M3 Max, 14 cores (10 performance + 4 efficiency), 36 GiB, macOS 15.7.3. #38's SP1 runs share it, under a lock that keeps the timed runs apart. Each proof waits up to 3 minutes for the 1-minute load average to fall below 3, and records the load averages next to its timing.
- **Toolchain:** `starkware-libs/stwo-cairo@2b1fb4ded5496fb2db4e8261cd5ee5ef6b710839`, with its `rust-toolchain.toml` (`nightly-2026-01-15`, rustc 1.94.0-nightly `86a49fd71`) and `.tool-versions` (`scarb 2.18.0`).
- **Prover:** `run_and_prove` from `crates/dev_utils`, built with `RUSTFLAGS="-C target-cpu=native"` and run with `--program_type executable --verify`. It uses the default production parameters (`crates/prover/src/prover.rs:409-424`): Blake2s channel, `log_blowup 1`, 70 queries, `pow_bits 26`, 96 bits. Every proof in the tables passed `--verify`.
- **Benchmarks:** the Scarb executable `bench/stwo-cairo/` (`stwo_unit_costs`), compiled like stwo-cairo's `proving` profile (`enable-gas = false`, `unsafe-panic = true`). Its arguments select the benchmark and N:
  - `blake2s`: N chained `core::blake::blake2s_compress` calls, `byte_count += 64` each;
  - `qm31`: N chained fused mul-adds `acc * b + c` on `core::qm31::qm31`, as `verifier_core/src/fields/qm31/opcode.cairo:50` does;
  - `m31`: N M31 values read from the arguments as `Array<core::qm31::m31>`, a range-checked deserialization, then summed.
- **One workaround:** at this commit, `run_and_prove --program_type executable` fails on any Scarb executable. The executable passes only the builtins it uses, but the prover reads all 11 public-segment pointers (the adapter's bootloader context), so it panics or reports "Constraints not satisfied". The script prepends a Standalone header that receives all 11 builtins and calls the executable's Bootloader entrypoint. That adds 15 steps (16 for the verifier, which uses `bitwise`). The tables give `scarb execute`'s step counts, without them.
- **Reproduce:** `python3 -I bench/stwo-cairo/run.py` clones the pin into `runs/stwo-cairo/`, writes proofs and logs to `runs/stwo-unit-costs/`, and prints every table below. It takes about an hour; neither `make check` nor CI runs it.

### Per-N measurements

Steps and max memory address come from `scarb execute --print-resource-usage`. Time, peak RSS and peak footprint come from `/usr/bin/time -l` over prove + verify. Trace cells are the prover's "Witness trace cells" (base + interaction trees); the 543,100,528-cell preprocessed tree is the same in every run. Peak footprint includes memory that macOS compressed or swapped out, so it is the figure that has to fit in 36 GiB.

**BLAKE2s compressions:**

| N | Steps | Max memory address | Trace cells | Prove + verify (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 57,413 | 53,655 | 164,617,856 | 6.41 | 8,957 | 15.4 | 12.3 | 2.8/3.8/6.1 | verified |
| 2^13 | 114,757 | 106,903 | 242,163,328 | 6.68 | 17,179 | 15.7 | 12.4 | 2.7/3.8/5.9 | verified |
| 2^14 | 229,445 | 213,399 | 397,254,272 | 10.40 | 22,062 | 17.3 | 14.3 | 2.8/3.7/5.7 | verified |
| 2^15 | 458,821 | 426,391 | 707,436,160 | 14.60 | 31,426 | 18.3 | 17.7 | 2.8/3.6/5.6 | verified |
| 2^16 | 917,573 | 852,375 | 1,327,799,936 | 27.60 | 33,245 | 19.9 | 26.9 | 2.9/3.8/5.5 | verified |
| 2^17 | 1,835,077 | 1,704,343 | — | — | — | — | 36.0 when stopped | 2.9/4.7/5.4 | out of memory |
| 2^18 | | | | | | | | | not attempted |

**Out of memory at N = 2^17:** the script stops a proof whose footprint passes 35 GiB. In an earlier pass without that limit, macOS killed the 2^17 proof at a peak footprint of 40.7 GiB, after its RSS reached 22.9 GiB. 2^18 needs more memory still and was not attempted. Proving them needs a larger machine, which needs explicit approval and a budget (AGENTS.md §2.6). The three points from 2^14 to 2^16 are enough for the fit.

**QM31 fused mul-adds:**

| N | Steps | Max memory address | Trace cells | Prove + verify (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 32,850 | 29,104 | 40,859,536 | 5.52 | 5,951 | 14.6 | 11.6 | 2.9/12.7/11.2 | verified |
| 2^13 | 65,618 | 57,776 | 44,971,920 | 6.40 | 10,253 | 14.6 | 11.6 | 2.9/10.9/10.6 | verified |
| 2^14 | 131,154 | 115,120 | 53,196,688 | 5.86 | 22,381 | 14.7 | 11.6 | 2.8/9.7/10.2 | verified |
| 2^15 | 262,226 | 229,808 | 69,646,224 | 6.29 | 41,689 | 14.6 | 11.6 | 2.7/7.9/9.5 | verified |
| 2^16 | 524,370 | 459,184 | 102,545,296 | 6.38 | 82,190 | 14.8 | 11.6 | 2.8/7.0/9.0 | verified |
| 2^17 | 1,048,658 | 917,936 | 168,343,440 | 6.75 | 155,357 | 15.2 | 12.1 | 2.8/6.3/8.6 | verified |
| 2^18 | 2,097,234 | 1,835,440 | 299,939,728 | 8.41 | 249,374 | 16.4 | 12.9 | 2.9/5.6/8.1 | verified |

**M31 ingestion:**

| N | Steps | Max memory address | Trace cells | Prove + verify (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Load avg (1/5/15 min) | Status |
|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 2^12 | 102,472 | 86,423 | 46,415,744 | 5.28 | 19,408 | 14.6 | 11.6 | 2.9/5.5/8.1 | verified |
| 2^13 | 204,872 | 172,439 | 56,090,496 | 5.72 | 35,817 | 14.7 | 11.6 | 2.6/4.9/7.6 | verified |
| 2^14 | 409,672 | 344,471 | 75,440,000 | 6.07 | 67,491 | 14.8 | 11.7 | 2.8/4.7/7.4 | verified |
| 2^15 | 819,272 | 688,535 | 114,139,008 | 7.11 | 115,228 | 15.1 | 11.9 | 2.8/4.5/7.1 | verified |
| 2^16 | 1,638,472 | 1,376,663 | 191,537,024 | 7.05 | 232,407 | 15.5 | 12.1 | 3.0/4.3/7.0 | verified |
| 2^17 | 3,276,872 | 2,752,919 | 346,333,056 | 9.19 | 356,569 | 17.3 | 13.2 | 2.7/4.1/6.6 | verified |
| 2^18 | 6,553,672 | 5,505,431 | 655,925,120 | 15.06 | 435,171 | 18.7 | 17.0 | 2.7/4.0/6.4 | verified |

### Fits: fixed cost plus per-op cost

Least squares over the points at N ≥ 2^14: 2^14 to 2^16 for BLAKE2s, 2^14 to 2^18 for the others.

| Per op | BLAKE2s compression | QM31 mul-add | M31 ingested |
|---|---:|---:|---:|
| Steps | 14 | 8 | 25 |
| Memory addresses | 13 | 7 | 21 |
| Trace cells, padded (base + interaction) | 18,932 | 1,004 | 2,362 |
| Prove + verify time, fit | 356.6 µs | 9.7 µs | 35.8 µs |
| Prove + verify time, marginal between the two largest N | 396.7 µs | 12.7 µs | 44.8 µs |
| Peak footprint | 270.9 KiB | 5.9 KiB | 22.6 KiB |
| **Fixed** (prove + verify; peak footprint) | 3.9 s; 9.8 GiB | 5.8 s; 11.4 GiB | 5.3 s; 11.0 GiB |

- **Fixed costs dominate.** Every run carries the preprocessed tree and the fixed-size lookup tables: about 37M base + interaction cells (87M with BLAKE2s's XOR tables), 4–6 s and 10–11 GiB of footprint.
- **Uncertainty.**
  - The QM31 and M31 timings stay nearly flat up to 2^16, so their per-op times rest mainly on 2^17 and 2^18. The marginal slopes between those two points are 25–30% above the fits.
  - The BLAKE2s 2^16 proof pushed about 5 GiB into swap, which may inflate it.
  - An earlier BLAKE2s pass, at a 1-minute load of 11–13, fitted 318 µs per compression. The range is about 320–400 µs.
- **Opcode mix per op:**
  - BLAKE2s: 1 `blake_compress_opcode` plus 13 ordinary instructions (loop, `byte_count` add with its range check).
  - QM31: 2 `qm_31_add_mul_opcode` instances, since the mul and the add are separate opcodes, plus 6 loop instructions.
  - M31: 25 ordinary instructions per value, with no special opcode.

### Rows and cells per op

Per BLAKE2s compression, at N = 2^16 with 65,536 `blake_compress_opcode` instances from `get_execution_resources`:

| Component | Columns (base + interaction) | Rows, inferred | Rows, padded (log size) | Cells, unpadded | Cells, padded |
|---|---:|---:|---:|---:|---:|
| `blake_compress_opcode` | 174 + 148 | 1 | 1 (2^16) | 322 | 322 |
| `blake_round` | 212 + 120 | 10 | 16 (2^20) | 3,320 | 5,312 |
| `blake_g` | 53 + 36 | 80 | 128 (2^23) | 7,120 | 11,392 |
| `triple_xor_32` | 21 + 20 | 8 | 8 (2^19) | 328 | 328 |
| **Total** | | **99** | **153** | **11,090** | **17,354** |

- **The inferred 99 rows and 11,090 cells hold.** The proof confirms the four log sizes, and the opcode count equals N.
- **Padding:** at N = 2^k, padding to powers of two makes it 153 rows and 17,354 cells.
- **Whole trace:** with the opcodes, memory and lookup components a compression also touches, it is 18,932 cells per compression (the fit above).
- **QM31 mul-add:** 2 `qm_31_add_mul_opcode` rows of 73 + 24 columns, 194 cells, unpadded and padded alike (2^19 rows at N = 2^18). With everything else it is 1,004 cells.
- **The extrapolation uses the padded whole-trace fits.** They are measured at N = 2^k, where every component is a power of two.

### Calibration against StarkWare's reference verifiers

The two CI commands, run from `stwo_cairo_verifier/` with `--profile proving`, execute in exactly the CI's step counts. The `stwo_cairo_verifier` run was then proved:

| Program | Steps | Prove + verify (s) | Steps/s | Peak RSS (GiB) | Peak footprint (GiB) | Trace cells | Load avg | Status |
|---|---:|---:|---:|---:|---:|---:|---|---|
| `stwo_cairo_verifier` (`qm31_opcode`) | 18,152,624 | 48.6 | 373,204 | 19.2 | 32.5 | 1,939,445,536 | 3.0/4.0/5.2 | verified |
| `stwo_circuit_verifier` (`qm31_opcode`) | 1,982,220 | executed only | | | | | | |

- **It agrees with StarkWare's "about 1 minute".** One M3 Max proves the Cairo verifier in 48.6 s, verification included.
- **It only just fits.** The peak footprint is 32.5 GiB, and swap grew from 22.5 to 33.2 GiB during the run, so the time may be inflated.
- **Steps/s depends on the opcode mix.** The verifier runs at 373k steps/s with 107 trace cells per step. At their largest N, the microbenchmarks run at 33k steps/s for BLAKE2s (1,352 cells per step), 249k for QM31 (126) and 435k for M31 ingestion (94).

### Extrapolation to TinyLlama (estimate)

From `docs/EVM-GAS-MODEL.md` §1 (65,967,346 leaf M31s, 519,784 OODS values, 501,197 columns, 518,489 internal hashes) and §3 (4,641,448 BLAKE2s compressions). Assumed multipliers:
- **BLAKE2s compressions** = leaf M31s / 16 + internal hashes. This reproduces §3's 4,641,448 exactly, and gives 525,665 for `head`.
- **QM31 mul-adds** = leaf M31s × OODS values / committed columns: one per queried value and sample point, as `accumulate_row_quotients` (`verifier_core/src/pcs/quotients.cairo:263`) does. That is 1.037 sample points per column on average, 1.016 for `head`.
- **Ingested felts** = leaf M31s + 4 × OODS values + 8 × internal hashes. Each costs one M31 ingestion, including the 32-bit hash words.
- **Costs add linearly.** Each segment is its own proof, so it pays once the largest of the three fixed costs.

| Workload | BLAKE2s | QM31 mul-adds | Ingested felts | Steps | Trace cells | Prove time: BLAKE2s / QM31 / ingestion / fixed (s) | Prove time | Peak footprint |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Per inference (23 proofs) | 4,641,448 | 68,413,759 | 72,194,394 | 2,417,152,085 | 3.29 × 10^11 | 1,655 / 663 / 2,582 / 132 | 5,033 s (~84 min) | see `head` |
| `head`, the largest segment | 525,665 | 8,227,075 | 8,533,469 | 286,512,720 | 3.85 × 10^10 | 187 / 80 / 305 / 6 | 578 s | ~377 GiB |

Checks for `head`:

| Check | Estimate | Bound |
|---|---:|---|
| Cairo memory, execution plus ingested arguments | 243,626,454 addresses (2^27.86) | `LOG_MEMORY_ADDRESS_BOUND = 29` (`crates/common/src/memory.rs:2`): 536,870,912 |
| Largest relation total uses, `MemoryAddressToId` | 1,305,486,035 (0.61 P) | below P (`crates/cairo-air/src/verifier.rs:63-66`) |
| Largest component | `memory_id_to_small` and `blake_g`, log size 27 (93.1M and 67.3M padded rows) | |
| Steps | 286,512,720 (2^28.09) | |

- **What the estimate covers.**
  - It counts hashing, quotient accumulation and ingestion only. It excludes AIR evaluation, the out-of-domain evaluation of SASH's 20k–68k-column constraints that a real SASH verifier adds, and the mixed-degree Merkle tree walk. It also excludes the GKR verification, channel draws and FRI folding.
  - It assumes per-op costs stay linear from the measured sizes (at most 2^23 rows) up to 2^27 rows. FFT and commitment costs grow as n log n, so the time is a floor.
- **Ingestion dominates,** at about 51% of the time, against 33% for BLAKE2s and 13% for QM31.
- **`head` passes both checks,** with a factor of 2.2 to spare on memory addresses and 1.6 on relation uses.
- **`head` doesn't fit this machine.** Its ~377 GiB footprint is about 10x its RAM. The per-inference time is what M3-Max-class cores would take with enough memory: an estimate, not a run on this machine. Splitting a segment's verification across several proofs would lower the footprint and add fixed costs.
- **Trace-length bound, unchecked context.** arXiv 2606.04311 (https://arxiv.org/pdf/2606.04311) reports a 2^29 + 1 trace-length bound for the S-two AIR. The `head` estimate, 2^28.09 steps with components at 2^27, would sit below it.

### Security parameters differ

stwo-cairo's default proves at 96 bits with 70 queries and `pow_bits 26`. SASH's segment proofs use 100 queries and 0 PoW bits, for 100 bits (`prover/src/util.rs:14-16`). Matching SASH's level in the recursion proof would change the query count or the PoW. The default code comment says the query count barely affects proving time.

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
  - `crates/adapter/src/lib.rs` (`PublicSegmentContext::bootloader_context`, all 11 builtins);
  - `.github/workflows/cairo-ci.yaml:136-153` (reference verifier step counts).
- S-two AIR trace-length bound (2^29 + 1, unchecked): https://arxiv.org/pdf/2606.04311
