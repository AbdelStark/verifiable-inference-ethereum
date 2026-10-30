# TinyLlama-1.1B SASH Sizing and EVM Gas Model

M1 has measured the grid path and the dev-mode settlement (§4). Every other gas figure here is an estimate until a milestone measures it: M3 the SP1 verifier and the aggregated settlement. §6 measures the SP1 unit costs of SASH's verifier primitives, §7 the 23 TinyLlama segments in the SP1 executor, and §8 the GPU proving of two of them.

## 1. TinyLlama-1.1B SASH proof sizes

Figures from the same SASH run as the fixture (`SPEC.md` §1.4), whose bundles `AbdelStark/hallmark`'s recipe (`make e2e-llm` at `675c99af66067d129dac30d6acc6612f4e392e68`, with patch 0006 applied before the prover build, `SPEC.md` §1.4) produced and `sp1/pins/tinyllama.sha256` pins: all 23 segments (`B0L0..B0L21` + `head`) of `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`) proved with `sg-ai-safety-hub/zk-inference-prover` (`eb95bfa887299d1637550228d42086c27cbb2b3b`, profile `y-attnout-w12`, `S = 32` tokens) plus the token binding (`sp1/patches/0006`, `SPEC.md` §1.1). The 69 bundle files are published as the public Hugging Face dataset [`0xchqrles/sash-tinyllama-bundles`](https://huggingface.co/datasets/0xchqrles/sash-tinyllama-bundles) at commit `7284cdba1353dfd441fb4a244aabd0c9bfb9dcd5`; `make bundles-download` (`sp1/scripts/tinyllama-bundles.sh`) fetches that commit anonymously into `runs/tinyllama` and checks every file against `sp1/pins/tinyllama.sha256`. `sp1-host sizes` recomputes every row from the bundles:

```bash
make bundles-download sp1-vendor
cd sp1 && cargo run --release --locked -p sp1-host --bin sp1-host -- sizes --bundles ../runs/tinyllama
```

| Quantity | `B0L0` (layer 0) | `B0L10` (typical interior) | `head` (LM head) | All 23 segments |
|---|---:|---:|---:|---:|
| Committed Merkle trees | 10 | 10 | 6 | 226 |
| Committed trace columns (`Fixed.sizes`: every tree but the 8-column composition tree) | 20,911 | 19,612 | 68,484 | 501,235 |
| Max trace `log_size` | 26 | 21 | 21 | 26 |
| FRI inner layers | 25 | 20 | 20 | 486 |
| Raw JSON bundle size (`fixed`, `claims` and `proof`) | 34,377,502 B | 30,399,070 B | 86,143,890 B | 761,159,331 B |
| STARK proof, stwo `size_estimate()` (in memory, not a wire format) | 12,960,104 B | 11,521,348 B | 34,419,240 B | 289,461,144 B |
| GKR proof, `serde_json` bytes (`Claims.gkr`) | 312,783 B | 312,662 B | 257,129 B | 7,136,116 B |
| Sampled OODS `QM31` values | 21,718 | 20,406 | 69,560 | 519,784 |
| Queried + witness leaf `M31`s (the committed trees' queried values and column witness) | 2,895,622 | 2,630,253 | 8,164,242 | 65,987,266 |
| Internal Merkle + FRI sibling hashes | 31,172 | 20,305 | 19,534 | 518,617 |

Patch 0006 changes the transcript of the two segments that publish a token vector, `B0L0` and `head`, so their re-proved proofs open other query positions and their queried values, sibling hashes and JSON sizes differ from the proofs without it; the other 21 segments' files are byte-identical. The bundles corrected four figures of the run's earlier summary: the per-segment JSON sizes were `proof.json` alone; `size_estimate()` is 40 B more per segment, the 40-byte `PcsConfig` nitroo's estimate counts; and the totals are 501,235 columns, not 501,197, and 486 FRI inner layers, not 485.

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

1. **`BLAKE2s-256` execution:** precompile `0x09` (`EIP-152`) implements 64-bit `BLAKE2f` (`BLAKE2b`), which cannot compute 32-bit `BLAKE2s`. Verifying the 23 segments runs `5,134,853` 64-byte compressions, counted by `sp1-host`'s `native-check` (§7): `4,898,316` in Merkle `hash_node` and `236,537` in the channel. At ~2,000 gas per compression in Solidity, that is **~10.27 billion gas**.
   - The analytic count, leaf M31s / 16 + sibling hashes, gives `4,642,821`: the counted Merkle hashes are 5.5% more, and the channel is extra. The analytic figure charges 1/16 of a compression per leaf M31 and one per sibling hash. The verifier hashes every node it computes, and each hash pays whole 64-byte blocks: a node's partial last block costs a full compression, every FRI leaf (one 16-byte `QM31`) costs one, and a mixed-degree node hashes its two children and its own column values.
2. **All 23 segments** (`296,597,260` B: STARK `size_estimate()` plus GKR bytes, ~85% non-zero):
   - Osaka: standard calldata is ~14.2 gas/byte, so ~4.21 billion gas. Adding ~10.27 billion of `BLAKE2s` execution gives ~14.48 billion, above the ~10.53 billion floor (~35.5 gas/byte). That is **at least ~863x** the `EIP-7825` cap.
   - Amsterdam: the 64 gas/byte floor alone is ~18.98 billion gas, **~1,131x** the cap.
   - Calldata floor alone under Osaka: ~10.53 billion gas, ~627x the cap.
3. **One segment, `B0L0`** (`13,272,887` B): the Osaka calldata floor alone is ~471.2 million gas, ~28.1x the cap.

## 4. Measured dev-mode settlement (Osaka)

M1 measures the grid path and the dev-mode settlement on Foundry 1.8.6 (`evm_version = "osaka"`). Two sources, which agree on `verifyAndSettle`:
- **anvil receipts:** `make anvil-demo` mines `DeployDevAnvil.s.sol` on `anvil --hardfork osaka --enable-tx-gas-limit`, before the SP1 run (§5), and prints this table and the settle transaction's `EIP-7623` breakdown, whose execution gas it checks against `dev_settle_execution_gas` in `contracts/script/anvil-settle.sh`.
- **forge snapshots:** `contracts/snapshots/*.json`, which `make check` regenerates and compares with the commit. Under `isolate = true`, the `verifyAndSettle` snapshot is the whole transaction and equals its receipt.

| Transaction (anvil receipt, Osaka, Foundry 1.8.6) | `gasUsed` | Gas limit | Calldata |
|---|---:|---:|---:|
| `SashModelRegistry` deployment | 291,740 | 379,262 | 1,292 B |
| `SashModelRegistry.enrollModel` | 92,053 | 127,147 | 164 B |
| `DevTranscriptSegmentVerifier` deployment | 223,223 | 290,189 | 837 B |
| `VerifiableInferenceSettlement` deployment | 874,235 | 1,136,505 | 4,305 B |
| `VerifiableInferenceSettlement.verifyAndSettle` | 218,380 | 301,636 | 7,972 B |

**The settle transaction under `EIP-7623`** (anvil receipt, Osaka, Foundry 1.8.6). Its calldata is 7,972 B: 4,050 zero and 3,922 non-zero bytes. The settle transaction is floor-bound under Osaka: `gasUsed` is the calldata floor, so execution is measured separately, as the top call's `gasUsed` in anvil's `trace_transaction` (Parity-style) trace of the mined transaction, which excludes intrinsic and calldata gas. Because the receipt and the `verifyAndSettle` snapshot stay at the floor, that execution figure is what `make anvil-demo` gates.

| Part | Gas |
|---|---:|
| Intrinsic | 21,000 |
| Standard calldata: 4 × 4,050 + 16 × 3,922 | 78,952 |
| Execution: `trace_transaction` top call `gasUsed` | 117,227 |
| Standard + execution: 21,000 + 78,952 + 117,227 | 217,179 |
| Floor: 21,000 + 10 × 4,050 + 40 × 3,922 | 218,380 |
| **`gasUsed`**: the larger of the two | **218,380** |

The floor binds by 1,201 gas, so cutting execution no longer lowers the settle transaction's cost; only less calldata does.

| Component of the settle execution (Osaka, Foundry 1.8.6) | Gas | Source |
|---|---:|---|
| `SashModelRegistry.getManifest` | 7,080 | forge snapshot |
| `SashGridVerifier.verifyGrid`: count and indices, `manifestRootsHash` (sha256 over 23 × 96 B), 22 residual links, prompt, teacher-forcing loop, answer and EOS | 40,000 | forge snapshot (`pure` harness) |
| `SashGridVerifier.computePublicDigest`: sha256 over the 23 claims (23 × 162 B), `sha256(tokIn)`, `sha256(tokOut)` | 28,961 | forge snapshot (`pure` harness) |
| Rest of `verifyAndSettle` (derived: 117,227 − the rows above): `runId` and prompt / answer hashes, the verifier calls, calldata decoding, the `settledRuns` cold `SSTORE` and the `InferenceVerified` event | 41,186 | derived |

The harness snapshots include their own ABI decoding, so the split between rows is approximate. The total is the measured execution.

### Amsterdam (provisional)

The same Osaka-compiled bytecode, mined on `anvil --hardfork amsterdam --enable-tx-gas-limit --code-size-limit 24576` from a Foundry nightly (`1.8.7-nightly`, commit `024702852`, 2026-10-27). These figures are provisional until Amsterdam activates on mainnet, and are taken from a nightly: Foundry 1.8.6's anvil doesn't apply Amsterdam gas (`EIP-7976`, `EIP-8037`, `EIP-2780`). #24 re-measures them on a stable Foundry release.

No receipt field separates state gas, so it is derived from the `EIP-8037` constants: 183,600 per new contract, 1,530 per byte of deployed code, 97,920 per new storage slot.

| Transaction (anvil receipt, Amsterdam, Foundry nightly `024702852`) | `gasUsed` | State gas (derived) | Rest | Osaka |
|---|---:|---:|---:|---:|
| `SashModelRegistry` deployment (1,095 B) | 1,902,912 | 1,858,950 | 43,962 | 291,740 |
| `SashModelRegistry.enrollModel` (3 new slots) | 350,025 | 293,760 | 56,265 | 92,053 |
| `DevTranscriptSegmentVerifier` deployment (784 B) | 1,420,693 | 1,383,120 | 37,573 | 223,223 |
| `VerifiableInferenceSettlement` deployment (3,763 B) | 6,032,117 | 5,940,990 | 91,127 | 874,235 |
| `VerifiableInferenceSettlement.verifyAndSettle` (1 new slot), computed | 525,208 | 97,920 | floor-bound | 218,380 |

The settle row is computed, not mined: 15,000 (`EIP-2780`) + 64 × 7,972 calldata bytes (`EIP-7976`) = 525,208, about 2.4x its Osaka cost. It is floor-bound, as the nightly's receipt was for the 7,236 B calldata before the claims carried `fixedHash` (478,104 = 15,000 + 64 × 7,236). The `enrollModel` and `VerifiableInferenceSettlement` deployment rows are that nightly's receipts for the bytecode before `fixedHash` (the settlement contract was 3,763 B; it is 3,773 B now); #24 re-mines every row. Deployments cost 6–7x more, almost all of it state gas.

## 5. Aggregated settlement (Osaka, measured)

M3 settles the TinyLlama run with its real PLONK proof: `contracts/fixtures/tinyllama.plonk.json` (`SPEC.md` §1.4), the aggregator's proof over the 23 segment proofs (§9.4), through `Sp1SashSegmentVerifier` and the v6.1.0 `SP1VerifierPlonk` compiled from the `sp1-contracts` submodule under the repository's settings (solc 0.8.38, `via_ir`, 200 runs, `evm_version = "osaka"`), with `allowDevVerifier = false` and no dev verifier deployed. Foundry 1.8.6. The sources agree, as in §4:
- **anvil receipts:** `make anvil-demo` mines `DeployAnvilSp1.s.sol` on the node of the dev run (§4) and prints this table and the settle's `EIP-7623` breakdown. It checks exactly one `InferenceVerified` with the §1.4 `runId` and `verifierKind == 1` (`ZKVM_PLONK`), the settle's receipt against the `verifyAndSettlePlonk` snapshot and the `EIP-7623` total, its execution against `sp1_settle_execution_gas` in `contracts/script/anvil-settle.sh`, and the receipt `< 600000`, M3's exit gate.
- **forge snapshots:** `verifyAndSettlePlonk` in `contracts/snapshots/VerifiableInferenceSettlement.json`, the whole transaction under `isolate = true`, and `verifyAggregatedSegments` in `contracts/snapshots/Sp1SashSegmentVerifier.json`, the adapter's frame.

| Transaction (anvil receipt, Osaka, Foundry 1.8.6) | `gasUsed` | Gas limit | Calldata | Zero | Non-zero |
|---|---:|---:|---:|---:|---:|
| `SP1VerifierPlonk` deployment (`contractName` `SP1Verifier`) | 1,991,431 | 2,588,860 | 8,992 B | 92 | 8,900 |
| `SashModelRegistry` deployment | 291,740 | 379,262 | 1,292 B | 129 | 1,163 |
| `SashModelRegistry.enrollModel` | 92,053 | 127,147 | 164 B | 95 | 69 |
| `Sp1SashSegmentVerifier` deployment | 243,275 | 316,257 | 1,410 B | 155 | 1,255 |
| `VerifiableInferenceSettlement` deployment | 877,331 | 1,140,530 | 4,305 B | 329 | 3,976 |
| `VerifiableInferenceSettlement.verifyAndSettle` | **520,898** | 761,813 | 8,932 B | 4,143 | 4,789 |

The settlement deployment costs 3,096 more than the dev run's (§4): with `allowDevVerifier = false` its constructor calls the segment verifier's `verifierKind()`, a cold call the dev run skips.

**The settle transaction under `EIP-7623`.** Its calldata is the dev settle's with the 32 B dev proof replaced by the 964 B `proofBytes` (`SPEC.md` §1.2), padded to 992 B: 8,932 B, 4,143 zero and 4,789 non-zero.

| Part | Gas |
|---|---:|
| Intrinsic | 21,000 |
| Standard calldata: 4 × 4,143 + 16 × 4,789 | 93,196 |
| Execution: `trace_transaction` top call `gasUsed` | 406,702 |
| Standard + execution: 21,000 + 93,196 + 406,702 | **520,898** |
| Floor: 21,000 + 10 × 4,143 + 40 × 4,789 | 253,990 |
| **`gasUsed`**: the larger of the two | **520,898** |

Unlike the dev settle, it is execution-bound, by 266,908 gas: the verifier's execution outweighs its calldata. It is 3.1% of the `EIP-7825` cap and 79,102 gas below the 600,000 exit gate.

| Component of the settle execution (Osaka, Foundry 1.8.6) | Gas | Source |
|---|---:|---|
| `SashModelRegistry.getManifest` | 7,080 | forge snapshot; its frame in the settle's trace |
| `SashGridVerifier.verifyGrid` | 40,000 | forge snapshot (`pure` harness, §4) |
| `SashGridVerifier.computePublicDigest` | 28,961 | forge snapshot (`pure` harness, §4) |
| `Sp1SashSegmentVerifier.verifyAggregatedSegments`, the adapter frame | 290,249 | forge snapshot; its frame in the settle's trace |
| of which `SP1VerifierPlonk.verifyProof` | 286,360 | the settle's trace |
| of which `PlonkVerifier.Verify`, the self-call that checks the PLONK proof | 282,990 | the settle's trace |
| Rest (derived: 406,702 − the four rows above): `runId` and the prompt / answer hashes, the `verifierKind()` call (150), calldata decoding, the `settledRuns` cold `SSTORE` and the `InferenceVerified` event | 40,412 | derived |

As in §4, the harness snapshots include their own ABI decoding, so the split between rows is approximate; the total is the measured execution.

**Both compiles of the verifier.** The repository compiles the vendored verifier under its own settings, so its bytecode is not the deployed `0xc3c6dDDAc8829b233Dc6536Ec024775a57b0AF2A`'s (`SECURITY.md` §6). Both verify the committed proof. Measured as a direct `verifyProof(programVKey, publicValues, proofBytes)` transaction on a local anvil fork of Hoodi (`anvil --fork-url` a public Hoodi RPC, `--hardfork osaka`, chain `560048`, block 3,918,454; both verifiers on the same fork, the repository's deployed there from `contracts/out`), execution from `trace_transaction`:

| Verifier | Runtime size | `verifyProof` execution | Measured |
|---|---:|---:|---|
| The repository's compile (the settlement's) | 8,966 B | 286,360 | Hoodi fork; equal to its frame in the anvil settle |
| The canonical deployment `0xc3c6…AF2A` | 10,125 B | 291,047 | Hoodi fork only |

The canonical deployment costs 4,687 more execution gas, so a settlement bound to it would cost about 525,585 gas on Osaka (derived, not measured). M4 measures it on Hoodi.

Amsterdam is not measured here: Foundry 1.8.6's anvil doesn't apply Amsterdam gas (§4). Its `EIP-7976` floor alone is 15,000 + 64 × 8,932 = 586,648 gas.

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

All 23 segments of the §1 run, re-proved with the token binding (`SPEC.md` §1.4), verified by the segment guest (`SPEC.md` §1.1) in the SP1 executor. This replaces #38's analytic extrapolation of the per-segment and total cycles and PGU; §6's unit costs stay.

- **SP1:** sp1-sdk `6.10.0` (circuit version `v6.1.0`), `LightProver`, `calculate_gas(true)`, `CYCLE_LIMIT` `12,000,000,000`.
- **Executor:** portable. sp1-core-executor 6.10.0 uses its native executor only on Linux x86_64 without the `profiling` feature (`build.rs`), so every macOS run is portable.
- **Machine:** Apple M3 Max, 14 cores, 36 GiB, macOS 15.7.3.
- **Program:** the committed segment guest (`SPEC.md` §1.1, patch 0006 and rule 8), the `segment-guest-elf` artifact of CI's x86_64 Docker build (#43's recipe): ELF sha256 `05dfd82121103f9448686fd567b86fc25c927c9d33a3c44625413e2f6b3eba6a`, vkey `bytes32` `0x003b73b0c998606408f61a261098a22a47e5edcbad107157e13763ef90c4997c` (`sp1/segment-guest.elf.sha256`, `sp1/segment-guest.vkey`).
- **SP1 options:** the defaults. `make sp1-execute` refuses to run if any variable `SP1CoreOpts::default()` reads is set (`MINIMAL_TRACE_CHUNK_THRESHOLD`, `GAS_TRACE_CHUNK_THRESHOLD`, `TRACE_CHUNK_SLOTS`, `GAS_TRACE_CHUNK_SLOTS`, `MEMORY_LIMIT`, `SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD`, `FULL_SIZE_SHARDS`): gas estimation treats each trace chunk as a shard, so the PGU depends on them.
- **Command** (about 10 minutes; `make check` runs it, then `make sp1-fixture-check`, when `make sp1-docker`'s ELF and the bundles are present; the `sp1-bundles` CI job runs its script, `sp1/scripts/sp1-execute.sh`, on the published bundles, on `B0L0`, `B0L10` and `head` on a PR (`SEGMENTS="B0L0 B0L10 head"`) and on all 23 on pushes to `main`, the weekly schedule and a manual run, with `sp1/scripts/sp1-fixture-check.sh` after it):

  ```bash
  make sp1-execute BUNDLES=runs/tinyllama OUT=<dir> ELF=<segment-guest>
  ```

  `ELF` is the Docker build: `make sp1-docker`'s output (the default) on an x86_64 host with the pinned image, or the `segment-guest-elf` artifact of the `sp1-workspace` CI job. `OUT` must be outside the repository or gitignored, and never the bundle directory or inside it. The target checks the 69 bundle files (`make bundles-check`), checks the ELF's sha256 and vkey against the committed ones, and runs `sp1-host execute` once per segment, each in its own process, so each peak RSS is that segment's. It checks every segment's 226 bytes of public values (`OUT/<tag>.public-values`) against native `check_segment`, and its `fixedHash` against `sp1/pins/tinyllama.fixed.sha256` (`PINS=write` writes that file). It prints the table below, then runs the controls. A segment that exits 1 or hits `CYCLE_LIMIT` stops the run.

Every segment exits 0, its public values equal native `check_segment`'s, and its `fixedHash` equals its pin. Bytes `0..162` of the 23 public values, the segment claims, equal the fixture's (`crates/sash-evm-primitives/src/tinyllama_fixture.rs`); #46 checks that: `sp1-host fixture-from-bundles --public-values OUT --bundles BUNDLES --prompt-len 25 --out PATH` renders `TinyLlamaFixture.sol` from the 23 public values and the token vectors of `B0L0.fixed.json` and `head.fixed.json`, after checking the token hashes and the grid, and `PATH`, never under `contracts/src`, is compared with the committed file. Cycles (`report.total_instruction_count()`) and PGU (`report.gas()`) depend only on the ELF and the input, with the defaults above, and `make sp1-execute` reproduces them exactly. Peak RSS depends on the executor build and the machine: `head` before patch 0006 peaked at 5,836,177,408 bytes in a `profiling` build of `sp1-host`.

| Segment | Cycles | PGU | Stdin bytes | Peak RSS (bytes) | `hash_node` compressions | Channel compressions | BLAKE2s share | Decode `Fixed` + `Claims` (cycles) | Decode proof (cycles) | Decode share |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `B0L0` | 2,064,635,701 | 1,930,854,534 | 14,141,959 | 5,047,156,736 | 225,007 | 10,109 | 24.36% | 62,346,827 | 118,808,389 | 8.77% |
| `B0L1` | 1,919,767,852 | 1,787,278,491 | 12,858,390 | 5,182,062,592 | 203,036 | 9,776 | 23.71% | 62,084,080 | 107,860,214 | 8.85% |
| `B0L2` | 1,912,038,302 | 1,781,008,887 | 12,900,561 | 5,134,548,992 | 202,226 | 9,774 | 23.72% | 62,098,723 | 106,322,852 | 8.81% |
| `B0L3` | 1,891,936,184 | 1,762,329,231 | 12,777,330 | 5,185,896,448 | 198,254 | 9,770 | 23.52% | 62,048,873 | 102,635,282 | 8.70% |
| `B0L4` | 1,888,667,956 | 1,758,807,596 | 12,738,291 | 5,119,115,264 | 197,298 | 9,766 | 23.45% | 62,064,793 | 102,376,245 | 8.71% |
| `B0L5` | 1,876,658,596 | 1,747,071,218 | 12,506,128 | 5,164,892,160 | 194,008 | 9,770 | 23.23% | 62,099,509 | 100,926,897 | 8.69% |
| `B0L6` | 1,879,309,325 | 1,749,776,699 | 12,558,072 | 5,191,663,616 | 194,158 | 9,766 | 23.21% | 62,083,444 | 100,945,926 | 8.67% |
| `B0L7` | 1,894,977,446 | 1,765,591,696 | 12,805,031 | 5,192,777,728 | 198,885 | 9,770 | 23.55% | 62,069,325 | 102,989,517 | 8.71% |
| `B0L8` | 1,895,457,260 | 1,765,205,480 | 12,699,707 | 5,147,279,360 | 198,430 | 9,772 | 23.50% | 62,106,794 | 103,791,116 | 8.75% |
| `B0L9` | 1,882,333,596 | 1,753,494,646 | 12,722,568 | 5,189,599,232 | 196,190 | 9,768 | 23.40% | 62,083,575 | 100,842,786 | 8.66% |
| `B0L10` | 1,879,852,673 | 1,750,956,986 | 12,689,128 | 5,190,615,040 | 195,433 | 9,768 | 23.35% | 62,080,301 | 100,433,288 | 8.65% |
| `B0L11` | 1,891,472,394 | 1,761,841,429 | 12,714,592 | 5,168,627,712 | 197,526 | 9,770 | 23.44% | 62,048,666 | 102,506,199 | 8.70% |
| `B0L12` | 1,869,530,320 | 1,739,625,044 | 12,487,194 | 5,213,044,736 | 192,269 | 9,768 | 23.12% | 62,086,383 | 99,153,629 | 8.62% |
| `B0L13` | 1,879,760,095 | 1,749,424,876 | 12,589,470 | 5,165,744,128 | 194,629 | 9,770 | 23.26% | 62,102,942 | 101,201,009 | 8.69% |
| `B0L14` | 1,886,794,943 | 1,757,305,463 | 12,731,427 | 5,202,034,688 | 196,843 | 9,770 | 23.42% | 62,078,467 | 102,083,044 | 8.70% |
| `B0L15` | 1,881,319,070 | 1,751,403,810 | 12,564,216 | 5,224,202,240 | 194,782 | 9,770 | 23.26% | 62,042,909 | 101,110,139 | 8.67% |
| `B0L16` | 1,890,677,664 | 1,760,862,151 | 12,773,562 | 5,193,580,544 | 197,933 | 9,770 | 23.50% | 62,086,471 | 102,346,568 | 8.70% |
| `B0L17` | 1,871,596,512 | 1,742,708,287 | 12,557,644 | 5,234,196,480 | 193,515 | 9,768 | 23.23% | 62,094,720 | 99,764,773 | 8.65% |
| `B0L18` | 1,880,141,730 | 1,749,829,741 | 12,528,379 | 5,182,406,656 | 194,310 | 9,770 | 23.22% | 62,088,869 | 101,005,287 | 8.67% |
| `B0L19` | 1,881,989,742 | 1,753,116,323 | 12,760,629 | 5,210,456,064 | 196,800 | 9,768 | 23.48% | 62,126,528 | 101,178,686 | 8.68% |
| `B0L20` | 1,893,171,887 | 1,763,557,815 | 12,800,757 | 5,177,262,080 | 198,508 | 9,770 | 23.53% | 62,071,031 | 102,684,109 | 8.70% |
| `B0L21` | 1,888,460,451 | 1,759,575,388 | 12,843,320 | 5,236,490,240 | 198,080 | 9,768 | 23.54% | 62,090,991 | 101,624,183 | 8.67% |
| `head` | 6,229,613,343 | 5,853,065,177 | 35,901,653 | 8,346,124,288 | 540,196 | 21,266 | 19.28% | 60,017,864 | 276,275,668 | 5.40% |
| **All 23** | **47,930,163,042** | **44,694,690,968** | **316,650,008** | **8,346,124,288** (largest) | **4,898,316** | **236,537** | **22.92%** | **1,426,102,085** | **2,538,865,806** | **8.27%** |

- **Stdin bytes:** the three buffers, `fixed.json` and `claims.json` as is and the proof re-encoded as `bincode` (`SPEC.md` §1.1).
- **BLAKE2s share:** the compressions `native-check` counts, times 2,139 cycles per compression (§6.1, `blake2s_compress`), over the segment's cycles. SP1 has no BLAKE2s precompile, so no syscall count gives it. `native-check` runs the same check natively, in a host build with stwo's `count-ops` feature (`sp1/patches/0005`): a counting `Blake2s256` adds `max(1, ⌈n / 64⌉)` compressions per hash of `n` bytes, at finalize. `hash_node` is the Merkle hasher, the channel is `Blake2sHasherGeneric` (mixes, draws and `mix_root`). The guest never enables the feature.
- **Decode share:** the cycles to decode the three buffers exactly as `check_segment` does (`Fixed` and `Claims` from JSON, the proof from `bincode`), in the `decode_fixed_claims` and `decode_proof` cycle-tracker regions of `stwo-bench`'s `decode_segment` operation (`decode-share`), over the segment's cycles. The segment guest has no regions, which would change its ELF and vkey. `stwo-bench` is the local build (`succinct` toolchain, rustc `1.96.0-dev`), not a committed program.
- **Over all 23 segments:** BLAKE2s is 22.9% of the cycles, decoding 8.3%: 3.0% for `Fixed` and `Claims`, 5.3% for the proofs. `Fixed` and `Claims` are 3.0% of the bundle bytes (23,157,213 of 761,159,331 B) and decode at ~62 cycles per byte, the proofs at ~8.7 per `bincode` byte (293,492,795 B). The rest, ~69%, is OODS constraint evaluation, DEEP quotients, FRI, GKR, the claims checks and the bookkeeping around them.

**Profiler cross-check.** Measured with the guest and the bundles before patch 0006 (`B0L10`'s cycles have since changed by 0.0002%, `head`'s by 0.24% and its compressions by 0.7%; not re-run). `sp1-host` built with sp1-sdk's `profiling` feature, run under `TRACE_FILE` with `TRACE_SAMPLE_RATE=1000` (one sample every 1,000 cycles; the default of 1 makes the trace far too large), then `sp1-host trace-share` counts the samples whose call stack holds `<blake2::Blake2sVarCore>::compress`:

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

**Controls on the real bundles.** `make sp1-execute` then writes nine modified copies under `OUT/controls/`, never in the bundle directory, and runs each natively and in the executor. Both sides give the same outcome. The two G9 forgeries (`SPEC.md` §2.1) find the honest vector's Horner image under patch 0006's `w` on both segments (`solved under the TokensMixed transcript's w`), so they are the attack on these proofs, and both reject. The same edit solved under upstream `eb95bfa`'s `w`, the review's proof of concept, rejects too (`NotAccepted` natively, exit 1 in the executor, at 201,342,754 and 370,794,015 cycles): upstream's `w` no longer gives the claimed Horner image on the re-proved bundles. Before patch 0006 the guest accepted that forgery on both (#69).

| Control | Segment and file | Native and executor | Executor cycles |
|---|---|---|---:|
| `queried_values[0][0]` + 1 mod P (the proof still decodes) | `B0L10` proof | exit 1, `NotAccepted` | 1,053,338,523 |
| `ao_out[0]` + 1 mod P (still decodable) | `B0L10` claims | exit 1, `NotAccepted` | 541,920,823 |
| `tok_in[0]` + P | `B0L0` fixed | exit 1, `TokenIdOutOfField` | 3,939,889 |
| `tok_out[0]` + P | `head` fixed | exit 1, `TokenIdOutOfField` | 12,112,997 |
| `model`'s last hex digit changed (no rule reads it) | `B0L10` fixed | exit 0, the unmodified public values but for `fixedHash`, `86c43bf7…89f08a5` against the pin `fa81937c…e19faf47`; the `manifestRootsHash` check rejects it (control 21, `SPEC.md` §2.1) | 1,879,914,432 |
| `tok_in[25]` = 450, `tok_in[1..=4]` solved to keep the Horner sum under the segment's own `w` | `B0L0` fixed | exit 1, `NotAccepted` | 201,342,847 |
| `tok_out[24]` = 450, `tok_out[0..=3]` solved to keep the Horner sum under the segment's own `w` | `head` fixed | exit 1, `NotAccepted` | 370,793,975 |
| `block` removed (`nn_pos_ok` holds without it) | `B0L10` fixed | exit 1, `NotSingleBlock` | 1,879,918,665 |
| `block` 0 to 1 (the head has no rotation, so `nn_pos_ok` holds) | `head` fixed | exit 1, `NotSingleBlock` | 6,230,007,156 |

Every row ran on the committed guest's Docker build (§7, Program). Cycles change with the ELF.

**Against the per-request limits** (one request per segment) **and in GPU time:**

| | `B0L0` | `B0L10` | `head` | All 23 |
|---|---:|---:|---:|---:|
| PGU / 1e12-PGU cap (#38's figure, unverified: sp1-sdk 6.10.0 defines no PGU cap, only the two limits below; §8.4 leaves it unassessed) | 0.19% | 0.18% | 0.59% | 4.47% (23 requests) |
| Cycles / sp1-sdk's 1e12-cycle `MAINNET_DEFAULT_CYCLE_LIMIT` | 0.21% | 0.19% | 0.62% | — |
| PGU / sp1-sdk's 1e9-PGU `DEFAULT_GAS_LIMIT` | 193.09% | 175.10% | 585.31% | — |
| GPU-hours at 2M PGU/s per GPU (unverified; §8 measures 8.0–8.3M PGU/s on one RTX PRO 6000, 1.49–1.55 GPU-hours for all 23) | 0.268 | 0.243 | 0.813 | 6.208 |

- sp1-sdk 6.10.0's limits (`network/mod.rs`) apply to a request that skips simulation and sets none (`get_execution_limits` in `network/prover.rs`). Every segment's PGU exceeds the 1e9 `DEFAULT_GAS_LIMIT`, so such a request must set its gas limit.
- The GPU rate derives from Succinct's Hypercube figure (an Ethereum block of about 400M PGU proved in under 12 s on 16 RTX 5090s). §8 replaces it with measured proofs of `B0L10` and `head`.
- #38 estimated the 23 segments at 3.75–4.92e10 cycles (3.45–4.51e10 PGU) from #41's synthetic runs. The measured 4.79e10 cycles (4.47e10 PGU) is near the top of that range. `head`, estimated at 3.32–5.73e9 cycles, measures 6.23e9, half of `CYCLE_LIMIT`.

## 8. Proving two TinyLlama segments on a GPU (measured) and the M3 go/no-go

Each figure is labelled measured or estimated. Estimates are computed from the measured proofs, §1, §7 and `docs/STARK-NATIVE-ROUTE.md`.

`B0L10` (a typical segment) and `head` (the largest) proved to SP1 compressed proofs, the mode M3's aggregator consumes, on one rented NVIDIA GPU. This replaces §7's unverified 2M PGU/s per GPU.

**Measured before patch 0006.** These proofs were made with the guest and the bundles before the token binding (#69), and no GPU run has been made since. On the re-proved bundles the committed guest's executor figures (§7) differ from those this section uses by +0.0002% in cycles and +0.004% in PGU for `B0L10`, +0.24% in cycles and +0.35% in PGU for `head`, and +0.016% in cycles and +0.034% in PGU over the 23 segments. Proving time scales with PGU (§8.2), so the figures below still apply within that margin, about 2.5 s on `head`'s 721.6 s. The cycles, PGU and Σ PGU quoted below are the ones the proofs were made with.

- **SP1:** sp1-sdk `6.10.0` with its `cuda` feature (`ProverClient::builder().cuda()`), which starts SP1's prebuilt `sp1-gpu-server` `v6.10.0` (release asset `sp1_gpu_server_v6.10.0_x86_64.tar.gz`, sha256 `20ee4b96fa6e5a2ce8a9da523ee42b75c698658ae1d4366e57b52bdd642f1bdb` on 2026-10-28; it links `libcudart.so.12`). Options are the defaults; the server sizes shards from the card's memory (`sp1-gpu/crates/prover_components/src/builder.rs:35-52`).
- **Program:** the segment guest before patch 0006, #43's Docker build: ELF sha256 `d8e6280fad7c667f5b07c18d891bf2444a1304046df2838b94161a77e6b52e28`, vkey `bytes32` `0x0025dac01e6b93eabfce48c8400bcd4c7bd401f61d2d82dfabfe1e2ee3eea2e2` (the committed guest, §7, has another ELF and vkey; `sp1-host prove` checks the committed ones).
- **Host:** Hugging Face Jobs flavor `rtx-pro-6000`, listed as 1× NVIDIA RTX PRO 6000 (96 GB), 23 vCPU, 256 GB RAM, $0.045833 per minute, on 2026-10-28:
  - GPU: NVIDIA RTX PRO 6000 Blackwell Server Edition, compute capability 12.0, 97,887 MiB, power limit 600 W; driver 580.178.04 (CUDA 13.0); container image `nvidia/cuda:12.8.1-base-ubuntu24.04` (CUDA 12.8 runtime);
  - CPU and RAM: the container sees the host's 2× Intel Xeon Platinum 8559C (96 cores, 192 threads) and 1,999 GB under KVM; the flavor allots 23 vCPU and 256 GB. Linux 6.12.103 (Amazon Linux 2023 kernel), x86_64;
  - two jobs, each building `sp1-host` from the pushed commit and proving `B0L10` then `head`: `6ae225053fb22a235dd43c80` (run 1, `3181b9a`, 1,215 s, $0.93) and `6ae22ab33fb22a235dd44633` (run 2, `5f6da0e`, 1,202 s, $0.92). Run 1's `sp1-host` printed every line of §8.1, then aborted (exit 134) as it dropped the CUDA client, which sp1-cuda 6.10.0 cleans up on a Tokio runtime that sp1-sdk's blocking API never enters; `5f6da0e` drops it inside one, and run 2 exits 0.
- **Command:** the one-tag `sp1-host prove` that #48's runs used, which no longer exists: §9.2 has the current batch command. On a Linux x86_64 host with an NVIDIA GPU of compute capability 8.0 or more, at least 24 GB of VRAM and a driver for CUDA 12.8, after `make sp1-vendor`:

  ```bash
  cd sp1 && cargo build --release --locked -p sp1-host --bin sp1-host
  target/release/sp1-host prove --elf <segment-guest> --bundle ../runs/tinyllama --tag B0L10 --out <dir>/B0L10.proof.bin
  ```

  That `sp1-host prove` checked everything it could on the CPU before it started the CUDA client: an `--out` named `<tag>.proof.bin` that did not exist yet (a symlink included), under the checkout's `runs/` or outside the checkout, never in the bundle directory; the ELF's sha256 against `sp1/segment-guest.elf.sha256`; its vkey, from a CPU setup, against `sp1/segment-guest.vkey`; and an executor run on the input, which had to exit 0 with the `fixedHash` in `sp1/pins/tinyllama.fixed.sha256`. It then built the CUDA client (which downloads `sp1-gpu-server` into `~/.sp1/bin` on first use and starts it; timed apart, so neither the download nor the start counts as setup or proving), set the key up and checked the GPU key's vkey again, proved in compressed mode, verified the proof, compared its public values with the executor's byte for byte and its `fixedHash` with the pin, then wrote the proof through a file it created only if nothing existed at that path. It printed each step's time, the proof's size and sha256, and `VmHWM` of `sp1-gpu-server`, the process that proves. GPU memory was sampled every second with `nvidia-smi --query-gpu=memory.used`. The bundles were checked against `sp1/pins/tinyllama.sha256` as it stood before patch 0006 on the host first, and the bundles and proofs were deleted from it afterwards.

### 8.1 Measured

Run 2, with run 1 in brackets.

| | `B0L10` | `head` |
|---|---:|---:|
| Cycles; PGU (§7 before patch 0006, reproduced by that `prove`'s executor run) | 1,879,848,647; 1,750,885,173 | 6,214,970,466; 5,832,830,141 |
| Stdin bytes | 12,689,128 | 35,640,318 |
| Executor run on the CPU, native executor (s) | 23.1 [23.3] | 64.7 [65.0] |
| Executor peak RSS, native executor (child process) (B) | 4,474,855,424 [4,474,544,128] | 4,725,551,104 [4,725,215,232] |
| Client build: server start (s) | 1.0 [1.0] | 1.0 [1.0] |
| Setup (s) | 11.5 [11.7] | 11.4 [11.6] |
| Prove, compressed (s) | **210.2** [215.4] | **721.6** [726.9] |
| Verify (s) | 0.0 [0.0] | 0.0 [0.0] |
| Compressed proof size (B) | 1,272,795 [1,272,795] | 1,272,795 [1,272,795] |
| Proof sha256 | `60bf1163…04c5a185` [`79ab83fb…fc2b5447`] | `ee8f7935…e67a98d6` [`c5bb0d2e…e9b2bb4a`] |
| Peak GPU memory used (MiB of 97,887, 1 s samples) | 26,509 [26,477] | 26,541 [26,477] |
| `sp1-gpu-server` `VmHWM` (kB) | 23,793,516 [23,767,916] | 25,671,472 [25,052,800] |
| `sp1-host prove` wall clock, all steps (`time -v`) | 4:08.04 [4:13.85] | 13:21.02 [13:27.15] |
| Proving rate (PGU/s) | 8,329,615 [8,128,529] | 8,083,190 [8,024,254] |
| Cost of the proof: prove time × $0.045833/min | $0.161 [$0.165] | $0.551 [$0.555] |
| Setup cost (one-time): setup time × $0.045833/min | $0.009 [$0.009] | $0.009 [$0.009] |
| The proof verifies; its public values equal the executor's byte for byte; `fixedHash` equals its pin | Yes [yes] | Yes [yes] |

- The compressed proof has the same size for both segments: it is one recursion proof of the segment's whole compress tree, whose size does not depend on the segment.
- The two runs' proofs of a segment differ, but both verify: the prover's proofs are randomized. Their public values are identical.
- Peak GPU memory is about the same for both segments, 27% of the card. The server sizes its shards from the card's memory, so a smaller card proves with smaller shards; no other card was measured.
- Setup is a one-time cost per server and ELF; it is priced apart from proving and left out of the per-inference cost. The executor run was that `sp1-host prove`'s own check, not part of proving; §9.2's `prove` drops it.

### 8.2 Extrapolation to the 23 segments (estimate)

Proving time scales with PGU at each segment's measured rate: all 23 segments take Σ PGU × (prove time / PGU). Σ PGU is 44,679,308,212 (§7 before patch 0006; 44,694,690,968 now, +0.034%), 25.518 × `B0L10`'s and 7.660 × `head`'s. The two rates differ by 3.1% in run 2 (120.05 and 123.71 s per 1e9 PGU) and 1.3% in run 1, so each column gives one end of the range:

| | At `B0L10`'s rate | At `head`'s rate |
|---|---:|---:|
| 23 segments on 1 GPU (s): 25.518 × `B0L10`'s prove time; 7.660 × `head`'s | 5,364 (89.4 min) [5,497] | 5,527 (92.1 min) [5,568] |
| GPU-hours | 1.490 [1.527] | 1.535 [1.547] |
| $ per inference, proving only: that time × $0.045833/min | **$4.10** [$4.20] | **$4.22** [$4.25] |
| Wall time on 23 GPUs in parallel: the slowest segment, `head` (3.331 × `B0L10`'s PGU) (s) | 700 [718] | 721.6 [726.9] (measured) |

Over both runs and both rates: 5,364–5,568 s and $4.10–4.25 per inference on one GPU. §7's 6.208 GPU-hours, at an unverified 2M PGU/s, was about 4× too high: one RTX PRO 6000 proves 8.0–8.3M PGU/s. PGU counts the work that SP1's prover network bills; the GPU's time per PGU may still vary with a segment's mix of chips, which the two segments sample.

**Aggregator (estimate; §9.3 measures it at `N = 1` and `N = 2`).** One guest verifies the 23 compressed proofs with `verify_sp1_proof` and commits the 32-byte `publicDigest`, as SP1's aggregation example does (sp1 `v6.10.0` `examples/aggregation`). Its RISC-V work measures 3,252,408 cycles and 3,083,538 PGU at `N = 23` in the executor (§9), 0.007% of §7's segment cycles and PGU. Each `verify_sp1_proof` is a deferred proof, checked by the recursion program like one node of a compress tree, so the aggregator's proof is one recursion layer over 23 proofs plus a short core execution. The server did not log how many core shards `B0L10`'s own compress tree folds, so no bound follows from it. Working figure: one `B0L10` proof, about 210 s and $0.16 on this GPU, which is a scale, not a bound.

**BN254 wrap (estimate; §9.3 measures it at `N = 1`).** One per inference and independent of the segments: SP1's shrink and wrap recursion, then gnark's PLONK prover over BN254 (`ProofMode::Plonk`, which `sp1-gpu-server` also serves). Not measured or estimated here. Its on-chain cost is §5's, measured: the `SP1VerifierPlonk` frame is 286,360 gas of a 520,898 gas settlement.

### 8.3 Against the S-two route and SASH's own proving

Both routes priced per inference on Hugging Face Jobs, each by its own method:

- **SP1:** two measured proofs, extrapolated to the 23 segments by PGU (§8.2). As a cross-check, the BLAKE2s compressions alone, §3's 5,134,853 at §6.1's 2,117 PGU each (10,870,483,801 PGU, 24.3% of §7's Σ PGU), priced at the same measured rates.
- **S-two:** unit costs (`docs/STARK-NATIVE-ROUTE.md`) scaled by §1's counts and §3's 5,134,853 BLAKE2s compressions.

| Per inference | SP1 GPU (§8.2), run 2 | S-two Cairo (`docs/STARK-NATIVE-ROUTE.md`, EPYC on `cpu-performance` at $0.031667/min) | SASH proving the run itself (#34) |
|---|---|---|---|
| Status | 2 segments measured, 21 extrapolated by PGU | Estimated: unit costs measured, no SASH verifier built | Measured |
| What it covers | The whole segment guest: decoding, `verify_composed` and the guest's rules | BLAKE2s, QM31 mul-adds, felt ingestion and the fixed cost; the rest only through the calibration scaling | Proving the 23 segments on CPU |
| BLAKE2s compressions alone (5,134,853), estimate | 1,305–1,345 s, $1.00–1.03: 10,870,483,801 PGU (§6.1's 2,117 per compression) at the measured rates | 1,109 s, $0.59: the EPYC's BLAKE2s fit | — |
| Time | 5,364–5,527 s on 1 GPU; 722 s on 23 GPUs, plus the aggregator and the wrap | 5,290–5,321 s modelled, 8,449 s scaled, on one 32-vCPU machine | about 31 min, build and verify included |
| Cost | $4.10–4.22 ($4.10–4.25 over both runs) | $2.79–2.81 modelled, $4.46 scaled | about $0.31 |
| `head` | 721.6 s, $0.55 (measured) | 616–618 s modelled, 982 s scaled ($0.33–0.52); needs ~439 GiB, so likely split | — |
| Security | SASH's ~100 conjectured bits inside; outer proof SP1's, then BN254 (`SECURITY.md`) | stwo-cairo's default: 96 bits, 70 queries, 26 PoW bits | ~100 conjectured bits |

- **SP1 against S-two:** SP1's measured $4.10–4.25 is 0.92–0.95× S-two's scaled $4.46 and 1.5× its modelled $2.79–2.81. On BLAKE2s alone, where S-two has native opcodes and SP1 no precompile, SP1 costs 1.7× as much ($1.00–1.03 against $0.59).
- **SP1 against SASH:** proving SASH's verifier in SP1 costs 13–14× SASH's own proving of the run (about $0.31).
- **No Cairo SASH verifier exists.** The S-two side leaves out the verifier glue: AIR evaluation of SASH's 20k–68k-column constraints, the mixed-degree Merkle walk, GKR, the channel and FRI folding, except through the calibration scaling. It is a lower bound with a rough correction, not a run.
- **The security parameters differ** (`docs/STARK-NATIVE-ROUTE.md`, "Security parameters differ"): matching SASH's 100 bits on the S-two side changes its query count or PoW.
- **Machines differ:** one GPU at $0.045833/min against one 32-vCPU CPU machine at $0.031667/min.

### 8.4 Go/no-go: GO on SP1

The thresholds below are the ones `docs/ROADMAP.md` records with M2's decision, GO on SP1.

| | Criterion | Figure | Threshold | Status |
|---|---|---|---|---|
| F1 | Every segment's §7 figures within half of SP1's limits | `head`, the largest: 6.23e9 cycles, 0.62% of sp1-sdk 6.10.0's 1e12-cycle `MAINNET_DEFAULT_CYCLE_LIMIT` (`network/mod.rs`); the network's 1e12-PGU request cap is not assessed: it is #38's figure, unverified, and sp1-sdk 6.10.0 defines no PGU cap (its 1e9-PGU `DEFAULT_GAS_LIMIT` is a default a request overrides, §7); 35,901,653 stdin bytes, 0.21% of the 16 GiB input region (`SP1_ZKVM_INPUT_REGION_SIZE` default `1 << 34`, sp1 `crates/zkvm/entrypoint/build.rs`); native executor child peak RSS 4,725,551,104 B (§8.1), 18.3% of its 24 GiB `DEFAULT_MEMORY_LIMIT` | ≤ 50% of each limit | Passes for cycles, input and RSS; PGU cap not assessed |
| F2 | `B0L10` and `head` prove without running out of memory | Both prove and verify, twice; peak 26,541 MiB of the GPU's 97,887 MiB, `sp1-gpu-server` `VmHWM` 25,671,472 kB of the 256 GB allotted | Both prove and verify | Passes |
| C1 | $ per inference | $4.61 (§9.4, measured): proving the 23 segments, 5,642.7 s, $4.31, plus the aggregator and its PLONK wrap over the 23 proofs, 395.1 s, $0.30 | ≤ $5, the order of the S-two route's scaled estimate ($4.46) | Meets it |
| C2 | Wall time per inference, 23 segments in parallel, aggregator and wrap included | `head`'s 738.5 s + the aggregator and its PLONK wrap, 395.1 s = 1,133.6 s, 18.9 min (§9.4, measured) | ≤ 1 h, about twice SASH's own 31 min | Meets it |
| C3 | §7's measured cycles against the ~1e10 estimate made before #45 | 4.79e10, 4.79× | ≤ 10× | Passes |

**GO**: F1 and F2 pass, and C1 and C2 meet their thresholds.

- F1 and F2 pass: every segment fits SP1's limits with a wide margin, and the largest proves in 27% of one card's memory.
- C1 and C2 meet the thresholds, measured on the 23 segments, the aggregator and its PLONK wrap (§9.4): $4.61 per inference on one GPU, about the S-two route's scaled estimate ($4.46), which still needs a Cairo SASH verifier that does not exist; 18.9 min with the segments in parallel. GO holds.
- Proving the segments costs about 14× SASH's own proving of the run ($4.31 against about $0.31), and BLAKE2s, with no SP1 precompile, is about a quarter of it.

## 9. M3: aggregator and wrap

### 9.1 The aggregator guest in the executor (measured)

The aggregator guest (`SPEC.md` §1.2, `sp1/aggregator-guest`) on the TinyLlama `modelId` and the segments' expected public values (`tinyllama_segment_public_values`), in the SP1 executor with deferred proof verification off and no segment proofs in stdin. The executor treats `verify_sp1_proof` as a no-op, but the guest still folds each call into its deferred-proofs digest with Poseidon2 in software (`sp1-zkvm-6.10.0` `src/syscalls/verify.rs`), so those cycles count.

- **SP1:** sp1-sdk `6.10.0`, `LightProver`, `calculate_gas(true)`, the default `SP1CoreOpts`; portable executor.
- **Machine:** Apple M3 Max, macOS, 2026-10-30.
- **Program:** the aggregator guest that `sp1-host`'s `build.rs` builds with the local `succinct` toolchain, not the Docker build pinned in `sp1/aggregator-guest.{elf.sha256,vkey}`. Cycles and PGU depend only on the ELF and the input; the `sp1-bundles` CI job prints the Docker build's on the 23 real public values (`sp1/scripts/sp1-fixture-check.sh`).
- **Command:** `cd sp1 && cargo test --locked -p sp1-host --test aggregator_guest -- --nocapture --test-threads=1`.

| `N` | Input | Commits | Cycles | PGU |
|---:|---|---|---:|---:|
| 1 | `B0L10`'s public values | `sha256(modelId \|\| be32(1) \|\| claim \|\| 0^32 \|\| 0^32)` | 1,248,544 | 1,115,967 |
| 23 | the 23 segments' public values | `SPEC.md` §1.4's `publicDigest` | 3,252,408 | 3,083,538 |

Each segment past the first adds 91,085 cycles and 89,435 PGU. That is execution only: the deferred-proof recursion that checks the `N` segment proofs runs when proving, and §8.2's estimate for it stands until the GPU run measures it.


### 9.2 The GPU pipeline

`sp1-host` proves on a local NVIDIA GPU through sp1-sdk's `cuda` feature (Linux x86_64, compute capability 8.0 or more, at least 24 GB of VRAM, a driver for CUDA 12.8). Each command checks everything it can on the CPU before it builds its one CUDA client, and builds and drops that client inside a Tokio runtime (sp1-cuda 6.10.0 cleans it up on the current runtime, §8).

- **`sp1-host prove --elf PATH --bundle DIR --tags B0L10,head|all --out-dir DIR [--then CMD]`** proves segments to compressed proofs with one client and one setup. On the CPU first: `--out-dir` exists, under the checkout's `runs/` or outside the checkout and never in the bundle directory; a requested tag's `<tag>.proof.bin` already there must decode (bincode `SP1ProofWithPublicValues`) to an `SP1Proof::Compressed` with the tag's expected public values (`tinyllama_segment_public_values`) and verify on the CPU against the segment vk, and that tag is skipped; any other file at a requested tag's `<tag>.proof.bin` refuses the run, which never overwrites, and other files in the directory are ignored; the ELF's sha256 and its vkey against `sp1/segment-guest.{elf.sha256,vkey}`; the remaining tags' bundle files. There is no executor run: the `sp1-bundles` CI job already shows that the same Docker-built ELF commits exactly those values on the real bundles. On the GPU, the setup's vkey is checked again, then for each remaining tag in order: prove compressed, verify, compare the public values byte for byte, write `<tag>.proof.bin` (created only if absent) and run `CMD` with `sh -c`, the proof's path as `$1`, so each proof is persisted before the next tag starts. A failing tag is reported and the next one starts; the exit code is 1 if any failed. It prints each tag's prove time, proof size and sha256, and `sp1-gpu-server`'s `VmHWM`.
- **`sp1-host aggregate --segment-elf PATH --aggregator-elf PATH --proofs DIR --tags …|all --mode compressed|plonk --out FILE [--fixture-out FILE] [--execute-only]`** proves the aggregator (`SPEC.md` §1.2). On the CPU first: both ELFs against their pins; the segment vk's `hash_u32` against the aggregator's `SEGMENT_VKEY` (a host guard, `SPEC.md` §2.1); each `<tag>.proof.bin` as above; `sash_aggregate::aggregate`'s digest, `SPEC.md` §1.4's `publicDigest` for the 23 tags in order; and the aggregator in the executor with the proofs in stdin, deferred proof verification on (SP1 verifies each against its own vk), committing that digest. `--execute-only` stops there and needs no GPU. Tags repeat only with `--mode compressed`; `--fixture-out` needs `--mode plonk` and the 23 tags in order. On the GPU: prove in `--mode`, verify with the client, require the public values to be the 32-byte digest, and in PLONK mode require `proofBytes` (`bytes()`) to be 964 bytes starting `0x5a093a2f` with nonce `0` and no TEE proof, `sp1-verifier`'s `PlonkVerifier` to accept them with its embedded `PLONK_VK_BYTES`, and the PLONK proof to verify with the sha256 public-values hash alone (`PlonkVerifier::verify` accepts any nonce and falls back to blake3, which `SP1VerifierPlonk` does not). It writes the bincode proof to `--out` (a new file under `runs/` or outside the checkout) and, in PLONK mode, prints `programVKey`, `publicValues` and `proofBytes` as 0x-hex JSON, which `--fixture-out` also writes.
- **`sp1-host aggregate-control --kind pv-byte|vk --aggregator-elf PATH [--segment-elf PATH --proofs DIR]`** runs binding control B1 or B2 (`SPEC.md` §2.1) at `N = 1` on `B0L10`, with `aggregate`'s pre-checks skipped. Only the control's half of the `(vk, sha256(public values))` pair differs from the honest run, and the other half is checked first: B1 puts `B0L10`'s real proof (verified on the CPU) in stdin and flips byte 10 of the `segmentValues`; B2 sets up the echo guest on the same client, proves it compressed on `B0L10`'s 226 expected public values, verifies that proof under its own vk and puts it in stdin with that vk. The aggregator is then proved in compressed mode, where the deferred-proof recursion runs. It exits 0 (pass) only if proving fails or the proof does not verify, and prints how; a verifying proof exits 1.
- **`sp1-host plonk-fixture --proof FILE --fixture FILE`** binds a PLONK proof file to the committed fixture, with no GPU: the file must decode (bincode `SP1ProofWithPublicValues`) to an `SP1Proof::Plonk` whose `proofBytes` pass `aggregate`'s PLONK checks above under `sp1/aggregator-guest.vkey`'s `bytes32` with the proof's own public values, and the fixture rendered from them must equal `--fixture` byte for byte. Since PLONK proofs are randomized, this is what ties the pinned `plonk/tinyllama.plonk.bin` to `contracts/fixtures/tinyllama.plonk.json`.
- **`sp1/scripts/upload-proof.sh <file> <dir>`** uploads one file to the dataset `$PROOFS_DATASET` as `<dir>/<name>` with `hf upload`. The token is read only from the `0600` file `HF_TOKEN_PATH` names; it refuses to run with `HF_TOKEN` set, since builds, `sp1-host` and `sp1-gpu-server` inherit their parent's environment.
- **`sp1/scripts/gpu-prove.sh smoke|all|wrap`** runs the pipeline from a checkout on any such host. Setup: apt, rustup, `sp1up --version v6.10.0`, `protoc`, `vendor-sash.sh`, `cargo build --release --locked -p sp1-host`, `huggingface_hub` 2.4.0, and `sp1-gpu-server` v6.10.0 fetched into `~/.sp1/bin` against §8's sha256 (the warm-up). It exports `SP1_PLONK_CIRCUIT_PATH` to the largest writable disk before any `sp1-host` starts: `sp1-gpu-server` inherits it (`sp1-cuda-6.10.0` `src/server.rs:35-43`), downloads the circuit into `$SP1_PLONK_CIRCUIT_PATH/v6.1.0/` (`sp1-prover-6.10.0` `src/build.rs:456-465`) and runs gnark natively, with no Docker (`sp1-gpu-server` links `sp1-prover` with `native-gnark`; measured in §9.3). Inputs: the 69 bundle files (`tinyllama-bundles.sh`; not in `wrap`, which proves no segment); the two Docker-built ELFs from the public dataset `0xchqrles/sash-tinyllama-sp1-proofs` under `elf/`, at the commit the script pins (`ELF_REVISION`), checked against the pins; and the proofs already in the dataset's `segments/`, so a re-run proves only the missing tags. `smoke` proves `B0L10` with `--then 'upload-proof.sh "$1" segments'`, then runs the honest `N = 1` compressed aggregation (`T1`), B1 and B2 (each capped at 6 min, a timeout reported as inconclusive; not run, and recorded as such, if the honest `N = 1` aggregation failed, since a control passes on any proving error), the `N = 2` compressed aggregation of `B0L10` twice (`T2`), and the `N = 1` PLONK wrap. `all` proves every missing segment the same way and stops, exiting non-zero, if one fails; if all 23 are in place, it wraps the 23 in PLONK with `--fixture-out`, prints the fixture and its sha256 (so a copy taken from the log can be checked byte for byte) before anything else can fail, and uploads that proof under `plonk/` (`upload-proof.sh` tries three times; a failure is reported, not fatal, so the results block still prints); only then does it run the compressed aggregation of the 23, capped at 6 min, for its timing only (the deferred recursion and compress, which the PLONK step's log does not separate from shrink and wrap; its proof is not kept and its outcome is a note, not a failure). Re-running `all` is the resume: it proves only the tags missing from `segments/`, then aggregates. `wrap` is the contingency: it downloads the circuit with `curl`, extracts it into `$SP1_PLONK_CIRCUIT_PATH/v6.1.0/` with the empty `.complete` marker that is all `sp1-prover` checks, and wraps the persisted `B0L10` proof. Each step waits until the previous step's `sp1-gpu-server` has exited and the GPU's `memory.used` is back within 256 MiB of its idle level (at most 60 s; past that, the step's peak is printed as an upper bound), then samples `memory.used` every second. Each PLONK step prints `df -h` before and after and samples `du` of `$SP1_PLONK_CIRCUIT_PATH` every second: that directory holds `sp1-prover`'s staging directory (`v6.1.0.incomplete.*`, with the tarball `artifacts.tar.gz` and then the tree extracted beside it) and the installed circuit, so its peak is the circuit's peak disk, unaffected by other tenants of a shared volume. From the tarball's size in each sample it prints the tarball's size, its download time (its first sample to the first at its final size) and the extraction time (from then to its deletion), at 1 s resolution. The bundles and proofs are deleted on exit, after the uploads have returned.
- **On Hugging Face Jobs,** a launcher kept outside the repository (it is account-specific) passes the write token only as a job secret; the job writes it to a `0600` file, unsets `HF_TOKEN`, sets `HF_TOKEN_PATH`, clones the pushed commit and runs the script:

  ```bash
  hf jobs run --flavor rtx-pro-6000 --timeout 45m --secrets HF_TOKEN -e COMMIT=<sha> -e MODE=smoke \
    nvidia/cuda:12.8.1-base-ubuntu24.04 bash -c "$(cat job.sh)"
  ```

  §9.4's run is the same with `--timeout 135m -e MODE=all`.
- **After the run,** with `REVISION` in `sp1/scripts/tinyllama-proofs.sh` set to the dataset commit that holds the 24 files, `PINS=write sp1/scripts/tinyllama-proofs.sh <empty dir>` writes `sp1/pins/tinyllama.proofs.sha256` from the dataset; `make proofs-download` then fetches and checks them anywhere, and `make sp1-proofs-check` (the `sp1-bundles` CI job's step) verifies the 23 segment proofs against the pinned segment vk and runs the aggregator over them in the executor (`aggregate --tags all --execute-only`), then requires the PLONK proof to render to the committed fixture (`plonk-fixture`). The fixture, `contracts/fixtures/tinyllama.plonk.json`, is the file `--fixture-out` wrote, printed by the job with its sha256.

### 9.3 The smoke run (measured)

`gpu-prove.sh smoke` on one segment, `B0L10`, end to end: the segment proof, the honest `N = 1` and `N = 2` compressed aggregations, binding controls B1 and B2 (`SPEC.md` §2.1) and the `N = 1` PLONK wrap.

- **Job:** Hugging Face Jobs `6ae4906ec78863f9c62e7813`, 2026-10-30, flavor `rtx-pro-6000` ($0.045833 per minute), image `nvidia/cuda:12.8.1-base-ubuntu24.04`, timeout 45 min, commit `325e19a`. It ran 897 s (5 s scheduling apart): **$0.69**. Every step exited 0.
- **Hardware:** NVIDIA RTX PRO 6000 Blackwell Server Edition, compute capability 12.0, 97,887 MiB, power limit 600 W, driver 580.178.04 (CUDA 13.0); 23 vCPU and 256 GB allotted (the container sees 192 threads and 1,999 GB); Linux 6.12.103, x86_64.
- **Software:** sp1-sdk `6.10.0`; `sp1-gpu-server` `v6.10.0`, binary sha256 `4043fe250c471515e144f4ae722b7ea3afd9bdc257609d3bcd9fea6247f74537` (from the tarball of §8's sha256); rustc 1.99.1.
- **ELFs:** the Docker builds of CI run 41051189040 (commit `96f8d3b`), published in the public dataset `0xchqrles/sash-tinyllama-sp1-proofs` at commit `29d003c1a7f6e94c6bcdf9e9473955cbf41dc768` (`ELF_REVISION`); both match `sp1/<guest>.elf.sha256`.
- **Dataset:** `B0L10`'s proof was uploaded as `segments/B0L10.proof.bin` right after it verified, dataset commit `2e49e17c4fb8abe1629b75da96928163417e6435`. §9.4's run reuses it.

| Step | Figures | Wall clock (`time -v`) |
|---|---|---:|
| Setup: apt, rustup, sp1up, vendored SASH | 31 s | |
| `cargo build --release --locked -p sp1-host` (guests included) | 101 s | |
| `huggingface_hub`, `sp1-gpu-server` warm-up, the 69 bundle files, the ELFs | 38 s; the first GPU step starts at 170 s | |
| `prove --tags B0L10` | setup 11.4 s; **prove 224.6 s** (§8.1: 210.2 s, +6.8%); proof 1,272,795 B, sha256 `f78e3a1f…fb810c745`, public values the expected ones; upload 1.1 s; `VmHWM` 24,209,192 kB; peak GPU 26,605 MiB | 4:00.02 |
| `aggregate --mode compressed --tags B0L10` (`T1`) | executor 1,248,544 cycles, 1,115,966 PGU, 0.5 s; setup 11.4 s; **prove 1.4 s**; proof 1,272,601 B; verifies, public values the digest `70b182fb…789ecfef`; `VmHWM` 12,305,928 kB | 0:17.27 |
| B1, `aggregate-control --kind pv-byte` | byte 10 `0x3c` to `0xc3`; **proving fails after 1.3 s** | 0:16.57 |
| B2, `aggregate-control --kind vk` | echo setup 11.5 s, echo prove 1.2 s, the echo proof verifies under its own vk with `B0L10`'s exact values; **proving fails after 1.3 s** | 0:16.30 |
| `aggregate --mode compressed --tags B0L10,B0L10` (`T2`) | executor 1,339,690 cycles, 1,205,382 PGU, 0.6 s; setup 11.5 s; **prove 1.5 s**; proof 1,272,601 B; verifies, public values the digest `341de548…09064a0e`; `VmHWM` 12,376,400 kB; peak GPU 12,707 MiB | 0:17.38 |
| `aggregate --mode plonk --tags B0L10` | setup 11.4 s; **prove 390.9 s**, stages below; `VmHWM` 69,257,968 kB; peak GPU at most 12,707 MiB (an upper bound, below) | 6:48.34 |

- **Binding controls.** B1 and B2 each produce no proof. `sp1-gpu-server` logs `task failed with fatal error: attempted to perform base field division 904710250/0` (B1; `655659029/0` for B2) and `Reduction task … failed`: an equality assertion of the recursion program fails, since SP1 compiles `assert_eq` into `SubF` then `DivF` of the difference by zero, which errors unless the difference is zero (`sp1-recursion-compiler-6.10.0` `src/circuit/compiler.rs:214-224`). The deferred-proof recursion rejects the mismatched `(vk, sha256(public values))` pair, as `SPEC.md` §1.2 says it must. The client returns `CudaClientError: The server returned an internal error … artifact not found`. Both ran beside the honest `N = 1` aggregation, which verifies, so neither pass comes from a broken setup.
- **The deferred recursion's slope.** `T2 - T1` is 0.1 s, at the 0.1 s resolution of the printed times: at `N = 1` and `N = 2`, the aggregation's compressed proof takes about 1.5 s and does not grow measurably with `N`. §9.4 measures `N = 23`.
- **The PLONK wrap.** `sp1-gpu-server` downloads the circuit and runs gnark natively on the job's CPU, with no Docker. Its log separates these stages; times are from its timestamps:
  - from the start of proving to the circuit download: 6.6 s; the log does not separate the core, compress, shrink and wrap stages in it;
  - circuit download (`v6.1.0-plonk.tar.gz`, 2,305,113,029 B), extraction and loading: about 82 s, one-time per host. The log does not separate them, so the download time alone is not measured here;
  - gnark PLONK prover over BN254, 27,576,375 constraints: 301.9 s, the solver's 9.9 s included;
  - gnark's verification: 1.8 ms.

  The extracted circuit takes 2.8 GB (`du -sh` of `$SP1_PLONK_CIRCUIT_PATH/v6.1.0`). The peak disk is not measured: this run's sampler read `df` of the 14 TB host volume the container's overlay shares, whose used space grew 48,292 MiB in that time, beyond what this step writes. The circuit's own peak is the tarball and the extracted tree side by side, about 5.1 GB (estimate). `gpu-prove.sh` now samples `du` of `$SP1_PLONK_CIRCUIT_PATH` and the staged tarball's size (§9.2), and §9.4 measures the peak disk and the download time. `VmHWM` of `sp1-gpu-server` reaches 66 GiB with gnark, within the 256 GB allotted.
- **GPU memory per step.** This run's sampler started as soon as the previous step's `sp1-host` exited, while that step's `sp1-gpu-server`, killed on the client's drop, could still hold GPU memory. `T1`'s 26,605 MiB is `B0L10`'s and is left out of the table. The PLONK step's 12,707 MiB equals `T2`'s, so it is an upper bound on the wrap's own peak, which may be lower. `gpu-prove.sh` now waits for the GPU to return to idle before a step samples (§9.2), and §9.4 measures the 23-segment wrap's own peak.
- **The PLONK proof.** bincode 4,103 B; `proofBytes` 964 B, selector `0x5a093a2f`, nonce 0; `programVKey` `0x00eb19e4…fe97747c` (`sp1/aggregator-guest.vkey`); `publicValues` the `N = 1` digest. It verifies:
  - with the SDK;
  - with `sp1-verifier`'s `PlonkVerifier` and with the sha256 public-values hash alone;
  - with the pinned v6.1.0 `SP1VerifierPlonk` in Foundry (forge 1.8.6, the repository's `foundry.toml`, a scratch test): `verifyProof` passes in 289,764 gas, and a flipped public-value byte reverts.

  The `N = 1` proof is not committed; §9.4's 23-segment one is.
- **§8.4's C1 and C2 still hold.**
  - C1: the `N = 1` aggregation and wrap take 390.9 s, $0.30, within the about $0.60 that C1 left for them. §8.2's $4.10–4.25 for the 23 segments was measured before patch 0006, and `B0L10` proved 6.8% slower here; scaled by that, the segments take 5,731–5,950 s, $4.38–4.55. Per inference, that is $4.38–4.55 + about $0.30 = $4.68–4.84, under $5.
  - C2: `head`'s 721.6 s, scaled by the same 6.8% to 771.0 s, + 390.9 s = 1,161.9 s, about 19.4 min, under 1 h.
  - Both carry the `N = 1` wrap and the 23-proof deferred recursion at the slope above; §9.4 measures `N = 23`.

### 9.4 The 23-segment run (measured)

`gpu-prove.sh all` on `rtx-pro-6000`, after #79's run: it downloads `B0L10`'s proof from `segments/`, proves the 22 other segments in order with one client and one setup, uploading each one as it verifies, then aggregates the 23 in PLONK mode with `--fixture-out`, uploads the PLONK proof under `plonk/`, and last aggregates them in compressed mode (timing only, capped at 6 min).

- **Timeout: 135 min (at most $6.19).** The rule: §8.2's upper end for the 22 remaining segments, 5,568 s less `B0L10`'s 210 s, 89 min, plus 1.5 × the rest from §9.3: the setup before the first GPU step (170 s), the `N = 1` PLONK step (6:48.34 wall clock, 408 s, the circuit download included) and 22 × the slope `T2 - T1` (0.1 s, 2.2 s), and the timing-only compressed step (`T1`'s 17.27 s wall clock plus the same 2.2 s, 19.5 s), 1.5 × 600 s = 900 s, 15 min. The sum, 104 min, is under 135 min, so the timeout stays at 135 min. The compressed step runs after the PLONK proof is uploaded and is capped at 6 min, so even at its cap it cannot cost the deliverable. The `N = 23` deferred recursion was unmeasured before this run, with the slope its only estimate. The run took 101.6 min.
- **Resume:** at most one job re-running `gpu-prove.sh all`, which proves only the tags missing from `segments/`, then aggregates. Its timeout is 1.5 × (the 170 s setup, the missing tags' §8.2 times and the aggregation, 408 s at `N = 1` or this run's measured figure), at most 60 min ($2.75). This run needed none.
- **Job:** Hugging Face Jobs `6ae4b8030e0338d1bb7cbc1a`, 2026-10-30, flavor `rtx-pro-6000` ($0.045833 per minute), image `nvidia/cuda:12.8.1-base-ubuntu24.04`, timeout 135 min, commit `643f859`. It ran 6,094 s (5 s scheduling apart), 101.6 min: **$4.66**. Every step exited 0.
- **Hardware and software:** §9.3's: the same GPU model, driver and allotment; sp1-sdk `6.10.0`, `sp1-gpu-server` `v6.10.0` from §8's tarball; the ELFs from the dataset at `ELF_REVISION`, checked against their pins.
- **Dataset:** each proof was uploaded right after it verified, one dataset commit each. The last, the PLONK proof's, is `d1cb00686f27b7491e71a28564e2ea490cd5621d`: `sp1/scripts/tinyllama-proofs.sh` pins it (`REVISION`), and `sp1/pins/tinyllama.proofs.sha256` was written from it (`PINS=write`). It holds the 23 segment proofs, 1,272,795 B each (`B0L10`'s from §9.3), and the PLONK proof.
- **Steps:** the setup, from apt to the proofs already in `segments/`, 174 s (§9.3: 170 s); `prove --tags all`, 1:31:07 wall clock for the 22 missing tags (setup 11.8 s, each upload 1.1–3.0 s, peak GPU 26,733 MiB, `sp1-gpu-server` `VmHWM` 25,103,984 kB); `aggregate --mode plonk`, 6:54.68; the timing-only `aggregate --mode compressed`, 0:22.69.

| Segment | PGU (§7) | Prove, compressed (s) | PGU/s |
|---|---:|---:|---:|
| `B0L0` | 1,930,854,534 | 239.7 | 8,055,296 |
| `B0L1` | 1,787,278,491 | 230.5 | 7,753,920 |
| `B0L2` | 1,781,008,887 | 236.3 | 7,537,067 |
| `B0L3` | 1,762,329,231 | 229.8 | 7,668,970 |
| `B0L4` | 1,758,807,596 | 238.3 | 7,380,645 |
| `B0L5` | 1,747,071,218 | 221.5 | 7,887,455 |
| `B0L6` | 1,749,776,699 | 226.1 | 7,738,950 |
| `B0L7` | 1,765,591,696 | 220.4 | 8,010,852 |
| `B0L8` | 1,765,205,480 | 228.9 | 7,711,688 |
| `B0L9` | 1,753,494,646 | 223.7 | 7,838,599 |
| `B0L10` (§9.3) | 1,750,956,986 | 224.6 | 7,795,890 |
| `B0L11` | 1,761,841,429 | 218.6 | 8,059,659 |
| `B0L12` | 1,739,625,044 | 213.0 | 8,167,254 |
| `B0L13` | 1,749,424,876 | 203.6 | 8,592,460 |
| `B0L14` | 1,757,305,463 | 216.6 | 8,113,137 |
| `B0L15` | 1,751,403,810 | 208.1 | 8,416,164 |
| `B0L16` | 1,760,862,151 | 207.5 | 8,486,083 |
| `B0L17` | 1,742,708,287 | 210.1 | 8,294,661 |
| `B0L18` | 1,749,829,741 | 222.6 | 7,860,870 |
| `B0L19` | 1,753,116,323 | 228.0 | 7,689,107 |
| `B0L20` | 1,763,557,815 | 234.6 | 7,517,297 |
| `B0L21` | 1,759,575,388 | 221.7 | 7,936,741 |
| `head` | 5,853,065,177 | 738.5 | 7,925,613 |
| **All 23** | **44,694,690,968** | **5,642.7** | **7,920,799** |

- **The 23 segments: 5,642.7 s, 94.0 min, $4.31** at $0.045833/min: 5,418.1 s for this run's 22 and §9.3's 224.6 s for `B0L10`. That is 7.92M PGU/s over §7's Σ PGU (7.38M–8.59M per segment), 1.3% above §8.2's extrapolated 5,364–5,568 s and 1.5% below §9.3's 5,731–5,950 s, which scaled §8.2 by `B0L10`'s 6.8%. `head` took 738.5 s (§8.1: 721.6 s, +2.3%). Each proof verified on the GPU host with its expected public values.
- **The aggregator over the 23 proofs, PLONK.** Before the GPU: both ELFs and vkeys, `SEGMENT_VKEY`, the 23 proofs verified on the CPU, the digest `SPEC.md` §1.4's `publicDigest` `cf52e19e…d067b3ef`, and the executor with the proofs in stdin (3,252,408 cycles, 3,083,538 PGU, §9.1's, 1.5 s). Setup 11.5 s, **prove 395.1 s** (§9.3's `N = 1`: 390.9 s). Times from `sp1-gpu-server`'s log:
  - from the start of proving to the circuit download: 9.3 s (`N = 1`: 6.6 s). The core execution, the deferred recursion over the 23 proofs, compress, shrink and wrap are all in it, and the log does not separate them;
  - the circuit download, extraction and loading: about 88 s, until gnark's prover starts (its end less its 296.6 s; `N = 1`: about 82 s). From the 1 s disk samples, the 2,305,113,029 B tarball downloaded in about 38 s and extracted in about 14 s; the remaining ~36 s is loading. One-time per host;
  - gnark's PLONK prover over BN254, 27,576,375 constraints: 296.6 s, the solver's 10.3 s included (`N = 1`: 301.9 s); its verification 1.9 ms;
  - peak circuit disk 6,764 MiB (`du` of `$SP1_PLONK_CIRCUIT_PATH`, the staging directory included; §9.3 estimated about 5.1 GB), 2.8 GB once extracted;
  - peak GPU memory 16,651 MiB, the step's own (the GPU was idle, at 0 MiB, when it started); `sp1-gpu-server` `VmHWM` 69,375,964 kB.
- **The PLONK proof.** bincode 4,103 B, sha256 `ca593bab…2154d680`, published as `plonk/tinyllama.plonk.bin`; `proofBytes` 964 B, selector `0x5a093a2f`, nonce 0; `programVKey` `0x00eb19e4…fe97747c` (`sp1/aggregator-guest.vkey`); `publicValues` the `publicDigest`. It verifies with the SDK and with `sp1-verifier`'s `PlonkVerifier` with the sha256 public-values hash alone. `contracts/fixtures/tinyllama.plonk.json` is the file `--fixture-out` wrote (2,128 B, sha256 `e27f39ff…e1cb3d16`, checked against the job's print), and `sp1-host plonk-fixture` binds the published proof to it (§9.2).
- **The deferred recursion, compressed (timing only).** `aggregate --mode compressed` over the 23 proofs: setup 11.5 s, **prove 4.7 s**, against `T1`'s 1.4 s: 3.3 s more, 0.15 s per added proof against §9.3's slope of 0.1 s, which estimated 3.6 s. Peak GPU 16,713 MiB, `VmHWM` 13,673,520 kB; the proof, 1,272,601 B, verifies with the 32-byte digest and is not kept. Against the PLONK step's 395.1 s, the recursion over 23 proofs is a few seconds; gnark dominates.
- **Per inference.** On one GPU: the 23 segments and the PLONK aggregation, 5,642.7 s + 395.1 s = 6,037.8 s, 100.6 min, **$4.61**, the circuit's one-time download included and the one-time setups (about 12 s each) and build left out. With the segments in parallel: `head`'s 738.5 s + 395.1 s = 1,133.6 s, **18.9 min**. §8.4's C1 and C2 hold on these figures.
- **Hugging Face spend in M3:** §9.3's job $0.69 and this one $4.66, $5.35 in all, against the $11.69 the M3 plan allows at the caps (~$5.8–6.5 expected); the contingency job was not used.
