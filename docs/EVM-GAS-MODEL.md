# TinyLlama-1.1B SASH Sizing and EVM Gas Model

M1 has measured the grid path and the dev-mode settlement (§4). Every other gas figure here is an estimate until a milestone measures it: M3 the SP1 verifier and the aggregated settlement. §6 measures the SP1 unit costs of SASH's verifier primitives, and §7 the 23 TinyLlama segments in the SP1 executor.

## 1. TinyLlama-1.1B SASH proof sizes

Figures from the same unpublished SASH run as the fixture (`SPEC.md` §1.4), whose bundles `AbdelStark/hallmark`'s recipe (`make e2e-llm` at `675c99af66067d129dac30d6acc6612f4e392e68`) regenerates byte for byte and `sp1/pins/tinyllama.sha256` pins: all 23 segments (`B0L0..B0L21` + `head`) of `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`) proved with `sg-ai-safety-hub/zk-inference-prover` (`eb95bfa887299d1637550228d42086c27cbb2b3b`, profile `y-attnout-w12`, `S = 32` tokens). `sp1-host sizes` recomputes every row from the bundles:

```bash
make sp1-vendor
cd sp1 && cargo run --release --locked -p sp1-host --bin sp1-host -- sizes --bundles ../runs/tinyllama
```

| Quantity | `B0L0` (layer 0) | `B0L10` (typical interior) | `head` (LM head) | All 23 segments |
|---|---:|---:|---:|---:|
| Committed Merkle trees | 10 | 10 | 6 | 226 |
| Committed trace columns (`Fixed.sizes`: every tree but the 8-column composition tree) | 20,911 | 19,612 | 68,484 | 501,235 |
| Max trace `log_size` | 26 | 21 | 21 | 26 |
| FRI inner layers | 25 | 20 | 20 | 486 |
| Raw JSON bundle size (`fixed`, `claims` and `proof`) | 34,830,250 B | 30,399,070 B | 85,547,307 B | 761,015,496 B |
| STARK proof, stwo `size_estimate()` (in memory, not a wire format) | 13,137,372 B | 11,521,348 B | 34,158,020 B | 289,377,192 B |
| GKR proof, `serde_json` bytes (`Claims.gkr`) | 312,711 B | 312,662 B | 257,094 B | 7,136,009 B |
| Sampled OODS `QM31` values | 21,718 | 20,406 | 69,560 | 519,784 |
| Queried + witness leaf `M31`s (the committed trees' queried values and column witness) | 2,940,131 | 2,630,253 | 8,099,813 | 65,967,346 |
| Internal Merkle + FRI sibling hashes | 31,151 | 20,305 | 19,427 | 518,489 |

The bundles corrected four figures of the run's earlier summary: the per-segment JSON sizes were `proof.json` alone; `size_estimate()` is 40 B more per segment, the 40-byte `PcsConfig` nitroo's estimate counts; and the totals are 501,235 columns, not 501,197, and 486 FRI inner layers, not 485.

## 2. Network rules

Osaka binds, and Amsterdam is tracked alongside it. Mainnet and Hoodi run Osaka. Sepolia has run Amsterdam since 2026-10-06, and Hoodi's Amsterdam activation was postponed on 2026-10-22 with no new date. The Amsterdam values are provisional until Amsterdam activates on mainnet.

| Rule | Osaka | Amsterdam (provisional) |
|---|---|---|
| `EIP-7825` gas cap (`16,777,216`) | total gas | all but state gas, so calldata and its floor still count (`EIP-8037`) |
| Calldata floor, gas per zero / non-zero byte | 10 / 40 | 64 / 64 (`EIP-7976`) |
| Intrinsic cost of a zero-value call | 21,000 | 12,000 + 3,000 (`EIP-2780`) |
| Code size limit | 24,576 B | 65,536 B (`EIP-7954`) |
| New storage slot (cold) | ~22,100 | ~110,000: 12,100 execution (`EIP-8038`) + 97,920 state gas (`EIP-8037`) |

- **Code size:** 24,576 B stays the project's limit under both forks.
- **Calldata (`EIP-7623`):** a transaction pays `max(standard + execution, floor)`, not their sum. Standard calldata costs 4 / 16 gas per zero / non-zero byte; the floor is the row above, plus the intrinsic cost.

## 3. Why raw SASH proofs cannot settle directly (estimates)

These figures derive from the §1 sizes and §7's compression counts, so they share their provenance.

1. **`BLAKE2s-256` execution:** precompile `0x09` (`EIP-152`) implements 64-bit `BLAKE2f` (`BLAKE2b`), which cannot compute 32-bit `BLAKE2s`. Verifying the 23 segments runs `5,133,478` 64-byte compressions, counted by `sp1-host`'s `native-check` (§7): `4,896,947` in Merkle `hash_node` and `236,531` in the channel. At ~2,000 gas per compression in Solidity, that is **~10.27 billion gas**.
   - The analytic count, leaf M31s / 16 + sibling hashes, gives `4,641,448`: the counted Merkle hashes are 5.5% more, and the channel is extra. The analytic figure charges 1/16 of a compression per leaf M31 and one per sibling hash. The verifier hashes every node it computes, and each hash pays whole 64-byte blocks: a node's partial last block costs a full compression, every FRI leaf (one 16-byte `QM31`) costs one, and a mixed-degree node hashes its two children and its own column values.
2. **All 23 segments** (`296,513,201` B: STARK `size_estimate()` plus GKR bytes, ~85% non-zero):
   - Osaka: standard calldata is ~14.2 gas/byte, so ~4.21 billion gas. Adding ~10.27 billion of `BLAKE2s` execution gives ~14.48 billion, above the ~10.53 billion floor (~35.5 gas/byte). That is **at least ~863x** the `EIP-7825` cap.
   - Amsterdam: the 64 gas/byte floor alone is ~18.98 billion gas, **~1,131x** the cap.
   - Calldata floor alone under Osaka: ~10.53 billion gas, ~627x the cap.
3. **One segment, `B0L0`** (`13,450,083` B): the Osaka calldata floor alone is ~477.5 million gas, ~28.5x the cap.

## 4. Measured dev-mode settlement (Osaka)

M1 measures the grid path and the dev-mode settlement on Foundry 1.8.6 (`evm_version = "osaka"`). Two sources, which agree on `verifyAndSettle`:
- **anvil receipts:** `make anvil-demo` mines `DeployDevAnvil.s.sol` on `anvil --hardfork osaka --enable-tx-gas-limit` and prints this table.
- **forge snapshots:** `contracts/snapshots/*.json`, which `make check` regenerates and compares with the commit. Under `isolate = true`, the `verifyAndSettle` snapshot is the whole transaction and equals its receipt.

| Transaction (anvil receipt, Osaka, Foundry 1.8.6) | `gasUsed` | Gas limit | Calldata |
|---|---:|---:|---:|
| `SashModelRegistry` deployment | 291,752 | 379,277 | 1,292 B |
| `SashModelRegistry.enrollModel` | 92,065 | 134,644 | 164 B |
| `DevTranscriptSegmentVerifier` deployment | 223,223 | 290,189 | 837 B |
| `VerifiableInferenceSettlement` deployment | 872,019 | 1,133,624 | 4,293 B |
| `VerifiableInferenceSettlement.verifyAndSettle` | 201,052 | 277,702 | 7,236 B |

**The settle transaction under `EIP-7623`** (anvil receipt, Osaka, Foundry 1.8.6). Its calldata is 7,236 B: 4,048 zero and 3,188 non-zero bytes.

| Part | Gas |
|---|---:|
| Intrinsic | 21,000 |
| Standard calldata: 4 × 4,048 + 16 × 3,188 | 67,200 |
| Execution: receipt − intrinsic − standard calldata | 112,852 |
| **`gasUsed`** | **201,052** |
| Floor: 21,000 + 10 × 4,048 + 40 × 3,188 | 189,000 |

The floor doesn't bind, but it is only 12,052 gas below `gasUsed`: cutting execution by more than ~12k would make the calldata floor bind.

| Component of the settle execution (Osaka, Foundry 1.8.6) | Gas | Source |
|---|---:|---|
| `SashModelRegistry.getManifest` | 7,080 | forge snapshot |
| `SashGridVerifier.verifyGrid`: count and indices, `manifestRootsHash` (sha256 over 23 × 64 B), 22 residual links, prompt, teacher-forcing loop, answer and EOS | 37,106 | forge snapshot (`pure` harness) |
| `SashGridVerifier.computePublicDigest`: sha256 over the 23 claims, `sha256(tokIn)`, `sha256(tokOut)` | 27,832 | forge snapshot (`pure` harness) |
| Rest of `verifyAndSettle` (derived: 112,852 − the rows above): `runId` and prompt / answer hashes, the verifier calls, calldata decoding, the `settledRuns` cold `SSTORE` and the `InferenceVerified` event | 40,834 | derived |

The harness snapshots include their own ABI decoding, so the split between rows is approximate. The total is the measured receipt.

### Amsterdam (provisional)

The same Osaka-compiled bytecode, mined on `anvil --hardfork amsterdam --enable-tx-gas-limit --code-size-limit 24576` from a Foundry nightly (`1.8.7-nightly`, commit `024702852`, 2026-10-27). These figures are provisional until Amsterdam activates on mainnet, and are taken from a nightly: Foundry 1.8.6's anvil doesn't apply Amsterdam gas (`EIP-7976`, `EIP-8037`, `EIP-2780`). #24 re-measures them on a stable Foundry release.

No receipt field separates state gas, so it is derived from the `EIP-8037` constants: 183,600 per new contract, 1,530 per byte of deployed code, 97,920 per new storage slot.

| Transaction (anvil receipt, Amsterdam, Foundry nightly `024702852`) | `gasUsed` | State gas (derived) | Rest | Osaka |
|---|---:|---:|---:|---:|
| `SashModelRegistry` deployment (1,095 B) | 1,902,912 | 1,858,950 | 43,962 | 291,752 |
| `SashModelRegistry.enrollModel` (3 new slots) | 350,025 | 293,760 | 56,265 | 92,065 |
| `DevTranscriptSegmentVerifier` deployment (784 B) | 1,420,693 | 1,383,120 | 37,573 | 223,223 |
| `VerifiableInferenceSettlement` deployment (3,763 B) | 6,032,117 | 5,940,990 | 91,127 | 872,019 |
| `VerifiableInferenceSettlement.verifyAndSettle` (1 new slot) | 478,104 | 97,920 | floor-bound | 201,052 |

The settle transaction is floor-bound: 15,000 (`EIP-2780`) + 64 × 7,236 calldata bytes (`EIP-7976`) = 478,104, about 2.4x its Osaka cost. Deployments cost 6–7x more, almost all of it state gas.

## 5. Aggregated settlement gas breakdown (Osaka)

One transaction carrying `SegmentClaim[23]`, the four token arrays and one wrapped SP1 proof: 7,492 B of ABI calldata with a 260 B Groth16-sized proof, 3,417 non-zero and 4,075 zero bytes. The SP1 verifier is an estimate until M3 measures it; the rest is the measured dev-mode settlement (§4).

| Component | Gas | Source |
|---|---:|---|
| Intrinsic tx gas | 21,000 | — |
| Standard calldata: 4 × 4,075 + 16 × 3,417 | 70,972 | computed |
| Dev-mode settle execution, 112,852, less the dev verifier's ~1,000 | ~111,850 | measured (§4: anvil receipt, Osaka, Foundry 1.8.6) |
| `Sp1SashSegmentVerifier` (BN254 wrap: ~270k Groth16, ~300k PLONK; chosen at M3 start) | ~270,000 | estimate, M3 measures it |
| **Total single-tx gas** | **~473,800** (~2.8% of `EIP-7825`) | estimate, M3 measures it |

The floor, 21,000 + 10 × 4,075 + 40 × 3,417 = 198,430 gas, is below the total, so it doesn't bind.

Under Amsterdam the floor alone is 15,000 + 64 × 7,492 = 494,488 gas, and the cold `SSTORE` rises to ~110,000 gas (§2), so the aggregated settlement would cost roughly 0.5–0.56M gas (estimate). M3 measures it.

## 6. SP1 unit costs of SASH's verifier (measured)

No BLAKE2s, BLAKE3 or M31 precompile exists in SP1 v6.10.0, so SASH's hashing and field arithmetic run as plain RV64IM instructions. This section measures them, and the guest's input decoding, one operation at a time. §7 measures the whole TinyLlama segments.

- **Setup:** sp1-sdk `6.10.0` (circuit version `v6.1.0`), the portable executor (sp1-sdk's `profiling` feature, which cycle-tracker regions need; the x86_64 JIT ignores them), on an Apple M3 Max with 36 GB, macOS. Cycles are `report.total_instruction_count()` and PGU `report.gas()`; both depend only on the guest and its input, not on the machine. `stwo-bench` links `zk-prover` for §7's decode regions, which changes its code generation; the tables are from that build.
- **Guest:** `sp1/stwo-bench` runs one operation `N` times over `black_box`ed inputs, in its own cycle-tracker region, with nitroo's Stwo (no `prover` feature). Each operation runs alone at two values of `N`; its cost is Δ/ΔN of the whole execution, and the region's Δ/ΔN is the cross-check. The loop and `black_box` overhead (a few cycles) is included.
- **Command** (about a minute after the release build; not in `make check` or CI):

  ```bash
  make sp1-vendor
  cd sp1 && cargo run --release --locked -p sp1-host --features profiling --bin unit-costs -- [CORPUS_DIR]
  ```

  `CORPUS_DIR` defaults to the gitignored `sp1/testdata/synthetic`. `make sash-synthetic` (Linux with the CUDA toolkit) writes it. Or download the `sash` CI job's artifact with `gh run download <run-id> -n sash-synthetic-corpus -D <dir>` and pass `<dir>/synthetic`. The command checks every file against `sp1/testdata/synthetic.sha256`, and prints every table below.

### 6.1 Unit costs

| Operation | N₁ / N₂ | Cycles at N₁ / N₂ | PGU at N₁ / N₂ | Cycles per op (Δ/ΔN) | PGU per op (Δ/ΔN) | Region cycles per op (cross-check) |
|---|---:|---:|---:|---:|---:|---:|
| `blake2s_compress` | 5,000 / 10,000 | 10,700,910 / 21,395,910 | 10,593,658 / 21,178,755 | 2,139 | 2,117 | 2,139 |
| `hash_node_leaf4` | 5,000 / 10,000 | 14,479,114 / 28,954,114 | 13,927,112 / 27,847,227 | 2,895 | 2,784 | 2,895 |
| `hash_node_leaf16` | 5,000 / 10,000 | 15,219,014 / 30,434,014 | 14,658,439 / 29,309,889 | 3,043 | 2,930 | 3,043 |
| `hash_node_leaf20` | 5,000 / 10,000 | 26,664,021 / 53,324,021 | 25,871,817 / 51,736,619 | 5,332 | 5,173 | 5,332 |
| `hash_node_leaf256` | 500 / 1,000 | 19,805,839 / 39,604,839 | 19,392,722 / 38,774,891 | 39,598 | 38,764 | 39,598 |
| `hash_node_leaf4096` | 32 / 64 | 20,032,376 / 40,015,672 | 19,652,479 / 39,239,979 | 624,478 | 612,109 | 624,478 |
| `hash_node_children` | 5,000 / 10,000 | 14,182,100 / 28,357,100 | 13,624,586 / 27,239,046 | 2,835 | 2,723 | 2,835 |
| `channel_draw` | 5,000 / 10,000 | 23,798,874 / 47,593,874 | 21,683,674 / 43,360,133 | 4,759 | 4,335 | 4,759 |
| `qm31_add` | 1,000,000 / 2,000,000 | 42,003,825 / 84,003,825 | 33,121,628 / 66,236,990 | 42.00 | 33.12 | 42.00 |
| `qm31_mul` | 200,000 / 400,000 | 56,803,828 / 113,603,828 | 50,890,360 / 101,774,370 | 284 | 254 | 284 |
| `qm31_mul_add` | 200,000 / 400,000 | 63,603,982 / 127,203,982 | 56,073,785 / 112,141,068 | 318 | 280 | 318 |
| `m31_qm31_mul` | 200,000 / 400,000 | 11,803,922 / 23,603,922 | 11,144,624 / 22,282,897 | 59.00 | 55.69 | 59.00 |
| `row_quotients` | 100 / 200 | 18,768,813 / 34,845,213 | 16,503,551 / 30,286,111 | 160,764 | 137,826 | 160,764 |

Per-op PGU comes from tight loops over small working sets. sp1-sdk 6.10.0's gas re-counts first memory reads at every gas-chunk boundary (`GAS_TRACE_CHUNK_THRESHOLD`, `sp1-core-executor` `opts.rs`). The real verifier's large working set pays more of those rows; §7's whole-segment runs include them.

`hash_node` is `Blake2sM31MerkleHasher::hash_node`, the hasher of SASH's proofs. It calls `update` once per 4-byte M31, then finalizes, so a node pays a whole 64-byte block for a partial one. Leaves of 4 and 20 M31s end in a partial block, and leaves of 16, 256 and 4,096 in a whole one. A least-squares fit of `per node + per block × ⌈4m / 64⌉ + per M31 × m` over the five sizes is exact at all five:

| Term | Cycles | PGU |
|---|---:|---:|
| Per node | 606 | 541 |
| Per 64-byte block | 2,240 | 2,194 |
| Bare compression (`blake2s_compress`, cross-check of the block term) | 2,139 | 2,117 |
| Per M31 (`update`) | 12.33 | 12.19 |
| Fit residual at 4 M31s (1 block) | -0.00% | +0.00% |
| Fit residual at 16 M31s (1 block) | +0.00% | +0.00% |
| Fit residual at 20 M31s (2 blocks) | -0.00% | -0.00% |
| Fit residual at 256 M31s (16 blocks) | +0.00% | -0.00% |
| Fit residual at 4,096 M31s (256 blocks) | -0.00% | +0.00% |
| `accumulate_row_quotients` per (column, sample) step (1,088 steps per row) | 148 | 127 |

- **BLAKE2s compression:** 2,139 cycles bare, below the S-two book's ~2,870 (measured before SP1 v6). Inside `hash_node`, a block costs 2,240 cycles, 5% more than the bare compression.
- **Per M31 in whole blocks:** 152 cycles, that is 140 for 1/16 of a block plus 12 for the per-value `update`. A partial block costs a whole one: a 4-M31 leaf (one QM31, an FRI leaf) costs 2,895 cycles, not 4 × 152.
- **DEEP quotients:** `accumulate_row_quotients` on a row of 1,024 columns, 64 of them sampled at a second point (1,088 (column, sample) steps over two sample points), costs 160,764 cycles: 148 per step, the per-row denominator inversions included.
- **QM31 arithmetic:** a QM31 mul costs 284 cycles, an M31 × QM31 mul 59.

### 6.2 Decoding the proof buffer

`StarkProof` decoded from `sound_aligned.proof.json` (23,594,585 B), and from its `bincode` 1.3.3 (`bincode::serialize`, fixed-width integers) and `postcard` 1.1.3 re-encodings. Each decode includes dropping the previous result. Each encoding round-trips: `serde_json::to_vec` of the decoded proof equals that of the proof parsed from `proof.json`. The guest's decoded commitment and queried-value counts match the host's. `serde_json` is 1.0.154, the version in `sp1/Cargo.lock`. SASH locks 1.0.151.

| Encoding | Size | Size / JSON | Cycles at N₁ / N₂ | Cycles per decode | Cycles per input byte | PGU per input byte | Region cycles per decode | Round trip |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| json | 23,594,585 B | 1.000 | 732,005,153 / 1,463,941,713 | 731,936,560 | 31.02 | 26.29 | 731,962,879 | yes |
| bincode | 9,099,625 B | 0.386 | 87,143,927 / 174,283,517 | 87,139,590 | 9.58 | 9.47 | 87,139,025 | yes |
| postcard | 10,489,334 B | 0.445 | 123,905,261 / 247,806,259 | 123,900,998 | 11.81 | 11.34 | 123,900,433 | yes |

`bincode` decodes the proof in 8.4x fewer cycles than JSON, and is the smallest encoding. `postcard`'s varints cost 1.4x `bincode`'s cycles at 1.15x its size. The guest's proof buffer is therefore `bincode` (`SPEC.md` §1.1). `Fixed` and `Claims` stay JSON: 23 `skip_serializing_if` fields in `compose.rs` stop them round-tripping through `bincode` or `postcard`, and they are under 1.2% of the synthetic corpus' bytes. §7 gives TinyLlama's share and its decode share.

## 7. TinyLlama segments in the SP1 executor (measured)

All 23 segments of the §1 run, verified by the segment guest (`SPEC.md` §1.1) in the SP1 executor. This replaces #38's analytic extrapolation of the per-segment and total cycles and PGU; §6's unit costs stay.

- **SP1:** sp1-sdk `6.10.0` (circuit version `v6.1.0`), `LightProver`, `calculate_gas(true)`, `CYCLE_LIMIT` `12,000,000,000`.
- **Executor:** portable. sp1-core-executor 6.10.0 uses its native executor only on Linux x86_64 without the `profiling` feature (`build.rs`), so every macOS run is portable.
- **Machine:** Apple M3 Max, 14 cores, 36 GiB, macOS 15.7.3.
- **Program:** the committed segment guest, #43's x86_64 Docker build: ELF sha256 `d8e6280fad7c667f5b07c18d891bf2444a1304046df2838b94161a77e6b52e28` (`sp1/segment-guest.elf.sha256`), vkey `bytes32` `0x0025dac01e6b93eabfce48c8400bcd4c7bd401f61d2d82dfabfe1e2ee3eea2e2` (`sp1/segment-guest.vkey`).
- **SP1 options:** the defaults. `make sp1-execute` refuses to run if any variable `SP1CoreOpts::default()` reads is set (`MINIMAL_TRACE_CHUNK_THRESHOLD`, `GAS_TRACE_CHUNK_THRESHOLD`, `TRACE_CHUNK_SLOTS`, `GAS_TRACE_CHUNK_SLOTS`, `MEMORY_LIMIT`, `SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD`, `FULL_SIZE_SHARDS`): gas estimation treats each trace chunk as a shard, so the PGU depends on them.
- **Command** (about 10 minutes; not in `make check` or CI, which have no bundles):

  ```bash
  make sp1-execute BUNDLES=runs/tinyllama OUT=<dir> ELF=<segment-guest>
  ```

  `ELF` is the Docker build: `make sp1-docker`'s output (the default) on an x86_64 host with the pinned image, or the `segment-guest-elf` artifact of the `sp1` CI job. `OUT` must be outside the repository or gitignored, and never the bundle directory or inside it. The target checks the 69 bundle files (`make bundles-check`), checks the ELF's sha256 and vkey against the committed ones, and runs `sp1-host execute` once per segment, each in its own process, so each peak RSS is that segment's. It checks every segment's 226 bytes of public values (`OUT/<tag>.public-values`) against native `check_segment`, and its `fixedHash` against `sp1/pins/tinyllama.fixed.sha256` (`PINS=write` writes that file). It prints the table below, then runs the controls. A segment that exits 1 or hits `CYCLE_LIMIT` stops the run.

Every segment exits 0, its public values equal native `check_segment`'s, and its `fixedHash` equals its pin. Bytes `0..130` of the 23 public values, the segment claims, equal the fixture's (`crates/sash-evm-primitives/src/tinyllama_fixture.rs`); #44 checks that. Cycles (`report.total_instruction_count()`) and PGU (`report.gas()`) depend only on the ELF and the input, with the defaults above, and `make sp1-execute` reproduces them exactly. Peak RSS depends on the executor build and the machine: the same `head` run peaks at 5,836,177,408 bytes in a `profiling` build of `sp1-host`.

| Segment | Cycles | PGU | Stdin bytes | Peak RSS (bytes) | `hash_node` compressions | Channel compressions | BLAKE2s share | Decode `Fixed` + `Claims` (cycles) | Decode proof (cycles) | Decode share |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `B0L0` | 2,071,629,039 | 1,937,178,087 | 14,318,984 | 4,997,529,600 | 227,781 | 10,106 | 24.56% | 62,333,235 | 119,899,130 | 8.80% |
| `B0L1` | 1,919,763,450 | 1,787,206,880 | 12,858,390 | 5,198,364,672 | 203,036 | 9,776 | 23.71% | 62,084,080 | 107,860,214 | 8.85% |
| `B0L2` | 1,912,034,441 | 1,780,940,400 | 12,900,561 | 5,176,098,816 | 202,226 | 9,774 | 23.72% | 62,098,723 | 106,322,852 | 8.81% |
| `B0L3` | 1,891,932,263 | 1,762,258,479 | 12,777,330 | 5,138,759,680 | 198,254 | 9,770 | 23.52% | 62,048,873 | 102,635,282 | 8.70% |
| `B0L4` | 1,888,664,794 | 1,758,736,539 | 12,738,291 | 5,209,079,808 | 197,298 | 9,766 | 23.45% | 62,064,793 | 102,376,245 | 8.71% |
| `B0L5` | 1,876,653,979 | 1,747,002,797 | 12,506,128 | 5,195,726,848 | 194,008 | 9,770 | 23.23% | 62,099,509 | 100,926,897 | 8.69% |
| `B0L6` | 1,879,306,142 | 1,749,705,703 | 12,558,072 | 5,157,322,752 | 194,158 | 9,766 | 23.21% | 62,083,444 | 100,945,926 | 8.67% |
| `B0L7` | 1,894,973,221 | 1,765,523,689 | 12,805,031 | 5,147,082,752 | 198,885 | 9,770 | 23.55% | 62,069,325 | 102,989,517 | 8.71% |
| `B0L8` | 1,895,453,178 | 1,765,137,124 | 12,699,707 | 5,168,660,480 | 198,430 | 9,772 | 23.50% | 62,106,794 | 103,791,116 | 8.75% |
| `B0L9` | 1,882,329,315 | 1,753,423,383 | 12,722,568 | 5,192,712,192 | 196,190 | 9,768 | 23.40% | 62,083,575 | 100,842,786 | 8.66% |
| `B0L10` | 1,879,848,647 | 1,750,885,173 | 12,689,128 | 5,206,212,608 | 195,433 | 9,768 | 23.35% | 62,080,301 | 100,433,288 | 8.65% |
| `B0L11` | 1,891,468,499 | 1,761,770,186 | 12,714,592 | 5,195,710,464 | 197,526 | 9,770 | 23.44% | 62,048,666 | 102,506,199 | 8.70% |
| `B0L12` | 1,869,526,469 | 1,739,552,445 | 12,487,194 | 5,191,041,024 | 192,269 | 9,768 | 23.12% | 62,086,383 | 99,153,629 | 8.62% |
| `B0L13` | 1,879,756,410 | 1,749,356,935 | 12,589,470 | 5,202,706,432 | 194,629 | 9,770 | 23.26% | 62,102,942 | 101,201,009 | 8.69% |
| `B0L14` | 1,886,790,798 | 1,757,238,334 | 12,731,427 | 5,183,340,544 | 196,843 | 9,770 | 23.42% | 62,078,467 | 102,083,044 | 8.70% |
| `B0L15` | 1,881,315,103 | 1,751,332,830 | 12,564,216 | 5,212,225,536 | 194,782 | 9,770 | 23.26% | 62,042,909 | 101,110,139 | 8.67% |
| `B0L16` | 1,890,673,727 | 1,760,790,089 | 12,773,562 | 5,202,771,968 | 197,933 | 9,770 | 23.50% | 62,086,471 | 102,346,568 | 8.70% |
| `B0L17` | 1,871,592,528 | 1,742,641,113 | 12,557,644 | 5,241,946,112 | 193,515 | 9,768 | 23.23% | 62,094,720 | 99,764,773 | 8.65% |
| `B0L18` | 1,880,137,954 | 1,749,758,692 | 12,528,379 | 5,189,140,480 | 194,310 | 9,770 | 23.22% | 62,088,869 | 101,005,287 | 8.67% |
| `B0L19` | 1,881,985,927 | 1,753,048,776 | 12,760,629 | 5,168,726,016 | 196,800 | 9,768 | 23.48% | 62,126,528 | 101,178,686 | 8.68% |
| `B0L20` | 1,893,167,685 | 1,763,486,029 | 12,800,757 | 5,172,822,016 | 198,508 | 9,770 | 23.53% | 62,071,031 | 102,684,109 | 8.70% |
| `B0L21` | 1,888,456,659 | 1,759,504,388 | 12,843,320 | 5,197,447,168 | 198,080 | 9,768 | 23.54% | 62,090,991 | 101,624,183 | 8.67% |
| `head` | 6,214,970,466 | 5,832,830,141 | 35,640,318 | 8,247,820,288 | 536,053 | 21,263 | 19.18% | 60,011,273 | 274,949,549 | 5.39% |
| **All 23** | **47,922,430,694** | **44,679,308,212** | **316,565,698** | **8,247,820,288** (largest) | **4,896,947** | **236,531** | **22.91%** | **1,426,081,902** | **2,538,630,428** | **8.27%** |

- **Stdin bytes:** the three buffers, `fixed.json` and `claims.json` as is and the proof re-encoded as `bincode` (`SPEC.md` §1.1).
- **BLAKE2s share:** the compressions `native-check` counts, times 2,139 cycles per compression (§6.1, `blake2s_compress`), over the segment's cycles. SP1 has no BLAKE2s precompile, so no syscall count gives it. `native-check` runs the same check natively, in a host build with stwo's `count-ops` feature (`sp1/patches/0005`): a counting `Blake2s256` adds `max(1, ⌈n / 64⌉)` compressions per hash of `n` bytes, at finalize. `hash_node` is the Merkle hasher, the channel is `Blake2sHasherGeneric` (mixes, draws and `mix_root`). The guest never enables the feature.
- **Decode share:** the cycles to decode the three buffers exactly as `check_segment` does (`Fixed` and `Claims` from JSON, the proof from `bincode`), in the `decode_fixed_claims` and `decode_proof` cycle-tracker regions of `stwo-bench`'s `decode_segment` operation (`decode-share`), over the segment's cycles. The segment guest has no regions, which would change its ELF and vkey. `stwo-bench` is the local build (`succinct` toolchain, rustc `1.96.0-dev`), not a committed program.
- **Over all 23 segments:** BLAKE2s is 22.9% of the cycles, decoding 8.3%: 3.0% for `Fixed` and `Claims`, 5.3% for the proofs. `Fixed` and `Claims` are 3.0% of the bundle bytes (23,156,855 of 761,015,496 B) and decode at ~62 cycles per byte, the proofs at ~8.7 per `bincode` byte (293,408,843 B). The rest, ~69%, is OODS constraint evaluation, DEEP quotients, FRI, GKR, the claims checks and the bookkeeping around them.

**Profiler cross-check.** `sp1-host` built with sp1-sdk's `profiling` feature, run under `TRACE_FILE` with `TRACE_SAMPLE_RATE=1000` (one sample every 1,000 cycles; the default of 1 makes the trace far too large), then `sp1-host trace-share` counts the samples whose call stack holds `<blake2::Blake2sVarCore>::compress`:

```bash
cd sp1 && cargo build --release --locked -p sp1-host --features profiling --bin sp1-host
TRACE_FILE=<tag>.trace.json TRACE_SAMPLE_RATE=1000 target/release/sp1-host execute --elf <segment-guest> --bundle ../runs/tinyllama --tag <tag>
target/release/sp1-host trace-share --trace <tag>.trace.json
```

| Segment | `hash_node` compressions | Channel compressions | Counted share (× 2,139 cycles) | Profiler samples under `compress` | Profiler share |
|---|---:|---:|---:|---:|---:|
| `B0L10` | 195,433 | 9,768 | 23.35% | 436,446 of 1,879,849 | 23.22% |
| `head` | 536,053 | 21,263 | 19.18% | 1,185,503 of 6,214,971 | 19.07% |

The two agree within 0.13 points: the bare compression's 2,139 cycles include a little loop and call overhead that the profiler counts outside `compress`.

**Controls on the real bundles.** `make sp1-execute` then writes five modified copies under `OUT/controls/`, never in the bundle directory, and runs each natively and in the executor. Both sides give the same outcome:

| Control | Segment and file | Native and executor | Executor cycles |
|---|---|---|---:|
| `queried_values[0][0]` + 1 mod P (the proof still decodes) | `B0L10` proof | exit 1, `NotAccepted` | 1,053,335,031 |
| `ao_out[0]` + 1 mod P (still decodable) | `B0L10` claims | exit 1, `NotAccepted` | 541,917,369 |
| `tok_in[0]` + P | `B0L0` fixed | exit 1, `TokenIdOutOfField` | 3,939,888 |
| `tok_out[0]` + P | `head` fixed | exit 1, `TokenIdOutOfField` | 12,112,996 |
| `model`'s last hex digit changed (no rule reads it) | `B0L10` fixed | exit 0, the unmodified public values but for `fixedHash`, `86c43bf7…89f08a5` against the pin `fa81937c…e19faf47`; #47's `manifestRootsHash` check rejects it | 1,879,910,406 |

**Against the per-request limits** (one request per segment) **and in GPU time:**

| | `B0L0` | `B0L10` | `head` | All 23 |
|---|---:|---:|---:|---:|
| PGU / 1e12-PGU cap (#38's figure, unverified; #48 confirms it) | 0.19% | 0.18% | 0.58% | 4.47% (23 requests) |
| Cycles / sp1-sdk's 1e12-cycle `MAINNET_DEFAULT_CYCLE_LIMIT` | 0.21% | 0.19% | 0.62% | — |
| PGU / sp1-sdk's 1e9-PGU `DEFAULT_GAS_LIMIT` | 193.72% | 175.09% | 583.28% | — |
| GPU-hours at 2M PGU/s per GPU (unverified; #48 measures it) | 0.269 | 0.243 | 0.810 | 6.205 |

- sp1-sdk 6.10.0's limits (`network/mod.rs`) apply to a request that skips simulation and sets none (`get_execution_limits` in `network/prover.rs`). Every segment's PGU exceeds the 1e9 `DEFAULT_GAS_LIMIT`, so such a request must set its gas limit.
- The GPU rate derives from Succinct's Hypercube figure (an Ethereum block of about 400M PGU proved in under 12 s on 16 RTX 5090s).
- #38 estimated the 23 segments at 3.75–4.92e10 cycles (3.45–4.51e10 PGU) from #41's synthetic runs. The measured 4.79e10 cycles (4.47e10 PGU) is near the top of that range. `head`, estimated at 3.32–5.73e9 cycles, measures 6.21e9, half of `CYCLE_LIMIT`.
