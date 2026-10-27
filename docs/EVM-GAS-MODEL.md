# TinyLlama-1.1B SASH Sizing and EVM Gas Model

Every gas figure here is an estimate until a milestone measures it: M1 measures the grid path and dev-mode settlement, M3 the SP1 verifier and the aggregated settlement.

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
| `EIP-7825` gas cap (`16,777,216`) | total gas | execution gas only (`EIP-8037`) |
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

## 4. Aggregated settlement gas breakdown (estimates, Osaka)

One transaction carrying `SegmentClaim[23]`, the four token arrays and one wrapped SP1 proof (7,492 B of ABI calldata with a 260 B Groth16-sized proof: 3,417 non-zero and 4,075 zero bytes). Standard calldata plus execution exceeds the ~177,000-gas floor, so the floor does not bind.

| Component | Gas (estimate) | Measured at |
|---|---:|---|
| Intrinsic tx gas | 21,000 | — |
| Calldata (standard, 7,492 B) | ~71,000 | M3 |
| `SashModelRegistry` manifest check (`sha256` + `SLOAD`) | ~5,500 | M1 |
| `SashGridVerifier`: 22 residual boundary links + token loop + EOS | ~14,000 | M1 |
| `publicDigest` token hashes: `sha256(tokIn)` + `sha256(tokOut)` | ~2,000 | M1 |
| `Sp1SashSegmentVerifier` (BN254 wrap: ~270k Groth16, ~300k PLONK; chosen at M3 start) | ~270,000 | M3 |
| `settledRuns[runId]` cold `SSTORE` + `InferenceVerified` event | ~25,600 | M1 |
| **Total single-tx gas** | **~409,100** (2.4% of `EIP-7825`) | M3 |

Under Amsterdam the cold `SSTORE` alone rises to ~110,000 gas (§2); M1 and M3 record the Amsterdam figures.
