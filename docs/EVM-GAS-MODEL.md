# TinyLlama-1.1B SASH Sizing and EVM Gas Model

M1 has measured the grid path and the dev-mode settlement (§4). Every other gas figure here is an estimate until a milestone measures it: M3 the SP1 verifier and the aggregated settlement. §6 measures the SP1 unit costs of SASH's verifier primitives and extrapolates them to TinyLlama.

## 1. TinyLlama-1.1B SASH proof sizes

Figures from the same unpublished SASH run as the fixture (`SPEC.md` §1.4), whose bundles `AbdelStark/hallmark`'s recipe (`make e2e-llm` at `675c99af66067d129dac30d6acc6612f4e392e68`) regenerates byte for byte and `sp1/pins/tinyllama.sha256` pins: all 23 segments (`B0L0..B0L21` + `head`) of `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`) proved with `sg-ai-safety-hub/zk-inference-prover` (`eb95bfa887299d1637550228d42086c27cbb2b3b`, profile `y-attnout-w12`, `S = 32` tokens). M2 reproduces them from the bundles.

| Quantity | `B0L0` (layer 0) | `B0L10` (typical interior) | `head` (LM head) | All 23 segments |
|---|---:|---:|---:|---:|
| Committed Merkle trees | 10 | 10 | 6 | 226 |
| Committed trace columns | 20,911 | 19,612 | 68,484 | 501,197 |
| Max trace `log_size` | 26 | 21 | 21 | 26 |
| FRI inner layers | 25 | 20 | 20 | 485 |
| Raw JSON bundle size | 33,816,971 B | 29,389,111 B | 84,613,678 B | 761,015,496 B |
| STARK proof, stwo `size_estimate()` (in memory, not a wire format) | 13,137,332 B | 11,521,308 B | 34,157,980 B | 289,376,272 B |
| GKR proof, `serde_json` bytes | 312,711 B | 312,662 B | 257,094 B | 7,136,009 B |
| Sampled OODS `QM31` values | 21,718 | 20,406 | 69,560 | 519,784 |
| Queried + witness leaf `M31`s | 2,940,131 | 2,630,253 | 8,099,813 | 65,967,346 |
| Internal Merkle + FRI sibling hashes | 31,151 | 20,305 | 19,427 | 518,489 |

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

These figures derive from the §1 sizes, so they share their provenance.

1. **`BLAKE2s-256` execution:** precompile `0x09` (`EIP-152`) implements 64-bit `BLAKE2f` (`BLAKE2b`), which cannot compute 32-bit `BLAKE2s`. At ~2,000 gas per 64-byte compression block in Solidity, the `4,641,448` blocks across 23 segments cost **~9.28 billion gas**.
2. **All 23 segments** (`296,512,281` B: STARK `size_estimate()` plus GKR bytes, ~85% non-zero):
   - Osaka: standard calldata is ~14.2 gas/byte, so ~4.21 billion gas. Adding ~9.28 billion of `BLAKE2s` execution gives ~13.49 billion, above the ~10.53 billion floor (~35.5 gas/byte). That is **at least ~804x** the `EIP-7825` cap.
   - Amsterdam: the 64 gas/byte floor alone is ~18.98 billion gas, **~1,131x** the cap.
   - Calldata floor alone under Osaka: ~10.53 billion gas, ~627x the cap.
3. **One segment, `B0L0`** (`13,450,043` B): the Osaka calldata floor alone is ~477.5 million gas, ~28.5x the cap.

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

## 6. SP1 unit costs of SASH's verifier (measured) and their extrapolation

No BLAKE2s, BLAKE3 or M31 precompile exists in SP1 v6.10.0, so SASH's hashing and field arithmetic run as plain RV64IM instructions. This section measures them, and the guest's input decoding, one operation at a time, then extrapolates to TinyLlama from the §1 counts. The extrapolation is analytic: #45 replaces it with measured segments.

- **Setup:** sp1-sdk `6.10.0` (circuit version `v6.1.0`), the portable executor (sp1-sdk's `profiling` feature, which cycle-tracker regions need; the x86_64 JIT ignores them), on an Apple M3 Max with 36 GB, macOS. Cycles are `report.total_instruction_count()` and PGU `report.gas()`; both depend only on the guest and its input, not on the machine.
- **Guest:** `sp1/stwo-bench` runs one operation `N` times over `black_box`ed inputs, in its own cycle-tracker region, with nitroo's Stwo (no `prover` feature). Each operation runs alone at two values of `N`; its cost is Δ/ΔN of the whole execution, and the region's Δ/ΔN is the cross-check. The loop and `black_box` overhead (a few cycles) is included.
- **Command** (about a minute after the release build; not in `make check` or CI):

  ```bash
  make sp1-vendor
  cd sp1 && cargo run --release --locked -p sp1-host --features profiling --bin unit-costs -- [CORPUS_DIR]
  ```

  `CORPUS_DIR` defaults to the gitignored `sp1/testdata/synthetic`. `make sash-synthetic` (Linux with the CUDA toolkit) writes it, or download the `sash` CI job's artifact with `gh run download`. The command checks every file against `sp1/testdata/synthetic.sha256`, and prints every table below.

### 6.1 Unit costs

| Operation | N₁ / N₂ | Cycles at N₁ / N₂ | PGU at N₁ / N₂ | Cycles per op (Δ/ΔN) | PGU per op (Δ/ΔN) | Region cycles per op (cross-check) |
|---|---:|---:|---:|---:|---:|---:|
| `blake2s_compress` | 5,000 / 10,000 | 10,700,909 / 21,395,909 | 10,593,536 / 21,178,632 | 2,139 | 2,117 | 2,139 |
| `hash_node_leaf16` | 5,000 / 10,000 | 15,219,012 / 30,434,012 | 14,658,417 / 29,309,867 | 3,043 | 2,930 | 3,043 |
| `hash_node_leaf256` | 500 / 1,000 | 19,820,820 / 39,634,820 | 19,403,998 / 38,797,476 | 39,628 | 38,787 | 39,628 |
| `hash_node_leaf4096` | 32 / 64 | 20,048,936 / 40,048,552 | 19,664,943 / 39,264,746 | 624,988 | 612,494 | 624,988 |
| `hash_node_children` | 5,000 / 10,000 | 15,152,278 / 30,297,278 | 14,531,503 / 29,052,717 | 3,029 | 2,904 | 3,029 |
| `channel_draw` | 5,000 / 10,000 | 23,798,873 / 47,593,873 | 21,683,602 / 43,360,061 | 4,759 | 4,335 | 4,759 |
| `qm31_add` | 1,000,000 / 2,000,000 | 42,003,825 / 84,003,825 | 33,121,608 / 66,236,969 | 42.00 | 33.12 | 42.00 |
| `qm31_mul` | 200,000 / 400,000 | 56,803,827 / 113,603,827 | 50,890,279 / 101,774,289 | 284 | 254 | 284 |
| `qm31_mul_add` | 200,000 / 400,000 | 63,603,973 / 127,203,973 | 56,073,680 / 112,140,962 | 318 | 280 | 318 |
| `m31_qm31_mul` | 200,000 / 400,000 | 11,803,920 / 23,603,920 | 11,144,602 / 22,282,875 | 59.00 | 55.69 | 59.00 |
| `row_quotients` | 100 / 200 | 18,765,786 / 34,839,186 | 16,501,965 / 30,282,751 | 160,734 | 137,808 | 160,734 |

`hash_node` is `Blake2sM31MerkleHasher::hash_node`, the hasher of SASH's proofs. It calls `update` once per 4-byte M31, so a leaf costs more than its compressions. A least-squares line through the three leaf sizes fits them exactly:

| Term | Cycles | PGU |
|---|---:|---:|
| Per M31 | 152 | 149 |
| Per node | 604 | 540 |
| Fit residual at 16 M31s | +0.00% | |
| Fit residual at 256 M31s | +0.00% | |
| Fit residual at 4096 M31s | +0.00% | |
| Bare compression ×1/16 per M31 (`blake2s_compress` 2,139 / 16) | 134 | |
| `accumulate_row_quotients` per (column, sample) step (1088 steps per row) | 148 | 127 |

- **BLAKE2s compression:** 2,139 cycles, below the S-two book's ~2,870 (measured before SP1 v6). Per M31, the leaf cost is 152 cycles: 134 for 1/16 of a compression, plus 18 for the per-value `update`.
- **DEEP quotients:** `accumulate_row_quotients` on a row of 1,024 columns, 64 of them sampled at a second point (1,088 (column, sample) steps over two sample points), costs 160,734 cycles: 148 per step, the per-row denominator inversions included.
- **QM31 arithmetic:** a QM31 mul costs 284 cycles, an M31 × QM31 mul 59.

### 6.2 Decoding the proof buffer

`StarkProof` decoded from `sound_aligned.proof.json` (23,594,585 B), and from its `bincode` 1.3.3 (`bincode::serialize`, fixed-width integers) and `postcard` 1.1.3 re-encodings. Each decode includes dropping the previous result. Each encoding round-trips: `serde_json::to_vec` of the decoded proof equals that of the proof parsed from `proof.json`. The guest's decoded commitment and queried-value counts match the host's. `serde_json` is 1.0.154, the version in `sp1/Cargo.lock`. SASH locks 1.0.151.

| Encoding | Size | Size / JSON | Cycles at N₁ / N₂ | Cycles per decode | Cycles per input byte | PGU per input byte | Region cycles per decode | Round trip |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| json | 23,594,585 B | 1.000 | 699,150,865 / 1,398,233,145 | 699,082,280 | 29.63 | 25.37 | 699,108,599 | yes |
| bincode | 9,099,625 B | 0.386 | 87,143,732 / 174,283,169 | 87,139,437 | 9.58 | 9.47 | 87,138,872 | yes |
| postcard | 10,489,334 B | 0.445 | 123,905,098 / 247,805,952 | 123,900,854 | 11.81 | 11.34 | 123,900,289 | yes |

`bincode` decodes the proof in 8.0x fewer cycles than JSON, and is the smallest encoding. `postcard`'s varints cost 1.4x `bincode`'s cycles at 1.15x its size. The guest's proof buffer is therefore `bincode` (`SPEC.md` §1.1). `Fixed` and `Claims` stay JSON: 23 `skip_serializing_if` fields in `compose.rs` stop them round-tripping through `bincode` or `postcard`, and they are under 1.2% of the corpus' bytes. #45 measures their decode share.

### 6.3 Extrapolation to TinyLlama

The model, per segment, from the §1 counts:
- **BLAKE2s leaves:** per M31 × leaf M31s + per node × bottom nodes. Bottom nodes (no children) are not in §1. They are bounded here by 100 per tree and 200 per FRI layer (first and inner): 6,200 for `B0L0`, 124,200 for all 23. That term is under 1% of the row.
- **BLAKE2s internal nodes:** `hash_node` (two children) × sibling hashes. Leaf M31s / 16 + sibling hashes = 65,967,346 / 16 + 518,489 reproduces §3's 4,641,448 compressions.
- **DEEP quotients:** steps = query positions × (sampled values + two-sample columns). `build_samples_with_randomness_and_periodicity` adds a periodicity sample to every column with two samples. On `sound_aligned`, which has 100 query positions, 20,534 sampled values and 1,157 two-sample columns, that is 2,169,100 steps. `100 × sampled values` gives 2,053,400, 5.3% low. For TinyLlama the model takes 100 positions, and two-sample columns = sampled values − columns, as on the synthetic proof, where no column has more than two samples. That gives 5.38e7 steps for all 23.
- **Decode:** the share of `proof.json` in the JSON bundle bytes is not in §1. It is taken from the synthetic corpus as 98.85% (all eight bundles' file sizes). The `proof` share is decoded at the measured cycles per byte × the encoded-to-JSON size ratio, and the `fixed` + `claims` share at JSON's cycles per byte. #45 records the real share.

**Cycles:**

| Cycles | `B0L0` | `B0L10` | `head` | All 23 |
|---|---:|---:|---:|---:|
| BLAKE2s, leaves: per M31 × leaf M31s + per node × bottom nodes (≤ 100 per tree, 200 per FRI layer) | 4.519e8 | 4.041e8 | 1.238e9 | 1.013e10 |
| BLAKE2s, internal nodes: `hash_node` (two children) × sibling hashes | 9.436e7 | 6.150e7 | 5.884e7 | 1.571e9 |
| DEEP quotients: per step × 100 × (sampled values + two-sample columns) | 3.328e8 | 3.132e8 | 1.044e9 | 7.954e9 |
| Decode `proof` as json | 9.905e8 | 8.608e8 | 2.478e9 | 2.229e10 |
| Decode `proof` as bincode | 1.235e8 | 1.073e8 | 3.089e8 | 2.778e9 |
| Decode `proof` as postcard | 1.755e8 | 1.526e8 | 4.392e8 | 3.950e9 |
| Decode `fixed` + `claims` (JSON) | 1.149e7 | 9.988e6 | 2.876e7 | 2.586e8 |
| FRI folds, field arithmetic only (bound) | 1.110e6 | 8.967e5 | 8.967e5 | 2.169e7 |
| **Measured parts, `proof` as json** | **1.881e9** | **1.650e9** | **4.847e9** | **4.220e10** |
| **Measured parts, `proof` as bincode** | **1.014e9** | **8.961e8** | **2.678e9** | **2.269e10** |
| **Measured parts, `proof` as postcard** | **1.066e9** | **9.413e8** | **2.808e9** | **2.386e10** |
| Unmeasured: OODS constraint evaluation, FRI fold domain points and inversions, the channel, GKR | — | — | — | — |

**PGU:**

| PGU | `B0L0` | `B0L10` | `head` | All 23 |
|---|---:|---:|---:|---:|
| BLAKE2s, leaves: per M31 × leaf M31s + per node × bottom nodes (≤ 100 per tree, 200 per FRI layer) | 4.426e8 | 3.958e8 | 1.213e9 | 9.923e9 |
| BLAKE2s, internal nodes: `hash_node` (two children) × sibling hashes | 9.047e7 | 5.897e7 | 5.642e7 | 1.506e9 |
| DEEP quotients: per step × 100 × (sampled values + two-sample columns) | 2.853e8 | 2.685e8 | 8.947e8 | 6.819e9 |
| Decode `proof` as json | 8.481e8 | 7.370e8 | 2.122e9 | 1.908e10 |
| Decode `proof` as bincode | 1.221e8 | 1.061e8 | 3.055e8 | 2.748e9 |
| Decode `proof` as postcard | 1.685e8 | 1.464e8 | 4.215e8 | 3.791e9 |
| Decode `fixed` + `claims` (JSON) | 9.840e6 | 8.552e6 | 2.462e7 | 2.214e8 |
| FRI folds, field arithmetic only (bound) | 9.785e5 | 7.903e5 | 7.903e5 | 1.912e7 |
| **Measured parts, `proof` as json** | **1.676e9** | **1.469e9** | **4.310e9** | **3.755e10** |
| **Measured parts, `proof` as bincode** | **9.503e8** | **8.379e8** | **2.494e9** | **2.122e10** |
| **Measured parts, `proof` as postcard** | **9.967e8** | **8.782e8** | **2.610e9** | **2.226e10** |
| Unmeasured: OODS constraint evaluation, FRI fold domain points and inversions, the channel, GKR | — | — | — | — |

**Against the prover network's 1e12-PGU request cap** (one request per segment) **and in GPU time.** The GPU-hour figures are unverified. They assume about 2M PGU/s per GPU, derived from Succinct's Hypercube figure (an Ethereum block of about 400M PGU proved in under 12 s on 16 RTX 5090s). #48 measures the real figure. The All 23 column sums 23 requests.

| Against the per-request cap (one request per segment) and GPU time | `B0L0` | `B0L10` | `head` | All 23 |
|---|---:|---:|---:|---:|
| `proof` as json: PGU / 1e12 cap | 0.17% | 0.15% | 0.43% | 3.76% |
| `proof` as json: GPU-hours at 2M PGU/s per GPU (unverified) | 0.233 | 0.204 | 0.599 | 5.216 |
| `proof` as bincode: PGU / 1e12 cap | 0.10% | 0.08% | 0.25% | 2.12% |
| `proof` as bincode: GPU-hours at 2M PGU/s per GPU (unverified) | 0.132 | 0.116 | 0.346 | 2.947 |
| `proof` as postcard: PGU / 1e12 cap | 0.10% | 0.09% | 0.26% | 2.23% |
| `proof` as postcard: GPU-hours at 2M PGU/s per GPU (unverified) | 0.138 | 0.122 | 0.362 | 3.092 |

**Unmeasured, for #45:** OODS constraint evaluation, the FRI folds' domain points and M31 inversions, the channel (one `draw_secure_felt` costs 4,759 cycles), the GKR verifier, and the claims checks. Only the FRI folds' field arithmetic (one QM31 mul, one M31 × QM31 mul and two QM31 adds per query per layer) has a cheap bound, in the tables.

**Cross-check against #41.** #41's scratch guest ran the whole `verify_composed`, JSON decoding of its three buffers included. It took 2,138,919,159 cycles on `sound_aligned` and 1,571,401,951 on `aligned_gkr5`. On the same bundles, the model's measured parts come to the figures below. Counts come from each `proof.json`. Its leaf M31s are the trees' queried values and column witness, plus 4 M31s per FRI leaf: one per FRI witness value, and a bound of 100 per layer for the queried ones.

| Bundle | Leaf M31s | Bottom nodes (bound) | Sibling hashes | Quotient steps | JSON bytes | BLAKE2s cycles | Quotient cycles | JSON decode cycles | Measured parts |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `sound_aligned` | 1,997,307 | 5,500 | 20,625 | 2,169,100 | 23,645,816 | 3.703e8 | 3.204e8 | 7.006e8 | 1.391e9 |
| `aligned_gkr5` | 1,217,451 | 5,500 | 20,093 | 1,424,700 | 15,807,784 | 2.498e8 | 2.105e8 | 4.684e8 | 9.286e8 |

The model's measured parts are 65% and 59% of those totals. The remaining 7.5e8 and 6.4e8 cycles are the unmeasured row's work plus model error, 54% and 69% of the measured parts. So the unmeasured row is of the same order as the measured parts on these bundles. TinyLlama's share stays unknown until #45.

**Reading.**
- With the proof as `bincode`, the measured parts of all 23 segments come to ~2.3e10 cycles (~2.1e10 PGU). That is already above the earlier ~10^10 estimate, before the unmeasured work.
- Decoding a JSON proof would add ~2.0e10 cycles, as much as everything else measured.
- The largest segment, `head`, is at about 0.25% of the 1e12-PGU request cap. No measured figure crosses #38's early-stop thresholds.
