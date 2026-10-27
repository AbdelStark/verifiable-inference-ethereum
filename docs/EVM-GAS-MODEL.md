# TinyLlama-1.1B SASH Sizing and EVM Gas Model

M1 has measured the grid path and the dev-mode settlement (§4). Every other gas figure here is an estimate until a milestone measures it: M3 the SP1 verifier and the aggregated settlement.

## 1. TinyLlama-1.1B SASH proof sizes

Figures from the same unpublished SASH run as the fixture (`SPEC.md` §1.4): all 23 segments (`B0L0..B0L21` + `head`) of `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`) proved with `sg-ai-safety-hub/zk-inference-prover` (`eb95bfa887299d1637550228d42086c27cbb2b3b`, profile `y-attnout-w12`, `S = 32` tokens). M2 reproduces them from the bundles.

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

**The settle transaction under `EIP-7623`.** Its calldata is 7,236 B: 4,048 zero and 3,188 non-zero bytes.

| Part | Gas |
|---|---:|
| Intrinsic | 21,000 |
| Standard calldata: 4 × 4,048 + 16 × 3,188 | 67,200 |
| Execution: receipt − intrinsic − standard calldata | 112,852 |
| **`gasUsed`** | **201,052** |
| Floor: 21,000 + 10 × 4,048 + 40 × 3,188 | 189,000 |

The floor doesn't bind, but it is only 12,052 gas below `gasUsed`: calldata, not execution, dominates the dev-mode settlement.

| Component of the settle execution (Osaka, Foundry 1.8.6) | Gas | Source |
|---|---:|---|
| `SashModelRegistry.getManifest` | 7,080 | forge snapshot |
| `SashGridVerifier.verifyGrid`: count and indices, `manifestRootsHash` (sha256 over 23 × 64 B), 22 residual links, prompt, teacher-forcing loop, answer and EOS | 37,106 | forge snapshot (`pure` harness) |
| `SashGridVerifier.computePublicDigest`: sha256 over the 23 claims, `sha256(tokIn)`, `sha256(tokOut)` | 27,832 | forge snapshot (`pure` harness) |
| Rest of `verifyAndSettle` (derived: 112,852 − the rows above): `runId` and prompt / answer hashes, the verifier calls, calldata decoding, the `settledRuns` cold `SSTORE` and the `InferenceVerified` event | 40,834 | derived |

The harness snapshots include their own ABI decoding, so the split between rows is approximate. The total is the measured receipt.

## 5. Aggregated settlement gas breakdown (Osaka)

One transaction carrying `SegmentClaim[23]`, the four token arrays and one wrapped SP1 proof: 7,492 B of ABI calldata with a 260 B Groth16-sized proof, 3,417 non-zero and 4,075 zero bytes. The SP1 verifier is an estimate until M3 measures it; the rest is the measured dev-mode settlement (§4).

| Component | Gas | Source |
|---|---:|---|
| Intrinsic tx gas | 21,000 | — |
| Standard calldata: 4 × 4,075 + 16 × 3,417 | 70,972 | computed |
| Dev-mode settle execution, 112,852, less the dev verifier's ~1,000 | ~111,850 | measured (§4) |
| `Sp1SashSegmentVerifier` (BN254 wrap: ~270k Groth16, ~300k PLONK; chosen at M3 start) | ~270,000 | estimate, M3 measures it |
| **Total single-tx gas** | **~473,800** (~2.8% of `EIP-7825`) | estimate, M3 measures it |

The floor, 21,000 + 10 × 4,075 + 40 × 3,417 = 198,430 gas, is below the total, so it doesn't bind.

Under Amsterdam the cold `SSTORE` alone rises to ~110,000 gas (§2); M1 and M3 record the Amsterdam figures.
