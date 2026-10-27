# Empirical TinyLlama-1.1B Sizing and EVM Gas Model

## 1. Primary-source TinyLlama-1.1B SASH proof measurements

Measured across all 23 segments (`B0L0..B0L21` + `head`) of `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`) proved with `sg-ai-safety-hub/zk-inference-prover` (`eb95bfa887299d1637550228d42086c27cbb2b3b`, profile `y-attnout-w12`, `S = 32` tokens):

| Quantity | `B0L0` (layer 0) | `B0L10` (typical interior) | `head` (LM head) | All 23 segments |
|---|---:|---:|---:|---:|
| Committed Merkle trees | 10 | 10 | 6 | 226 |
| Committed trace columns | 20,911 | 19,612 | 68,484 | 501,197 |
| Max trace `log_size` | 26 | 21 | 21 | 26 |
| FRI inner layers | 25 | 20 | 20 | 485 |
| Raw JSON bundle size | 33,816,971 B | 29,389,111 B | 84,613,678 B | 761,015,496 B |
| Packed binary STARK size | 13,137,332 B | 11,521,308 B | 34,157,980 B | 289,376,272 B |
| Binary GKR proof size | 312,711 B | 312,662 B | 257,094 B | 7,136,009 B |
| Sampled OODS `QM31` values | 21,718 | 20,406 | 69,560 | 519,784 |
| Queried + witness leaf `M31`s | 2,940,131 | 2,630,253 | 8,099,813 | 65,967,346 |
| Internal Merkle + FRI sibling hashes | 31,151 | 20,305 | 19,427 | 518,489 |

## 2. Ethereum consensus ceilings (`EIP-7825` & `EIP-7623`)

1. **`EIP-7825` (Fusaka):** $\text{MAX\_TX\_GAS} = 2^{24} = 16,777,216$ gas per transaction.
2. **`EIP-7623` (Pectra):** Calldata gas floor $= 21,000 + 10 \times (\text{zero\_bytes} + 4 \times \text{nonzero\_bytes})$.
   - For packed binary field/hash data (~85% non-zero bytes), effective floor is $10 \times (0.15 + 3.40) = 35.5$ gas/byte.
   - Single raw segment `B0L0` (`13,450,043` B): $\approx 477,497,526$ calldata gas (**28.5x** over `EIP-7825`).
   - Full 23-segment raw bundle (`296,512,281` B): $\approx 10,526,206,975$ calldata gas (**627x** over `EIP-7825`).
3. **`BLAKE2s-256` execution cost:** Ethereum precompile `0x09` (`EIP-152`) implements 64-bit `BLAKE2f` (`BLAKE2b`), which cannot compute 32-bit `BLAKE2s`. At ~2,000 gas per 64-byte compression block in Yul/Solidity, `4,641,448` blocks across 23 segments cost **~9.28 billion gas**.

## 3. Two-layer settlement gas breakdown

| Component | Aggregated 1-proof mode | Batched 23-proof mode |
|---|---:|---:|
| Intrinsic tx gas | 21,000 | 21,000 |
| Calldata (`SegmentClaim[23]` + tokens + Groth16 proof(s)) | ~64,000 (~4.6 KiB) | ~182,000 (~13.1 KiB) |
| `SashModelRegistry` manifest check (`sha256` + `SLOAD`) | ~5,500 | ~5,500 |
| `SashGridVerifier`: 22 residual boundary links + token loop + EOS | ~14,000 | ~14,000 |
| `publicDigest` token hashes: `sha256(tokIn)` + `sha256(tokOut)` | ~2,000 | ~2,000 |
| `Sp1SashSegmentVerifier` (BN254 Groth16 pairing `0x08`) | ~270,000 | ~6,210,000 |
| `settledRuns[runId]` cold `SSTORE` + `InferenceVerified` event | ~25,600 | ~25,600 |
| **Total single-tx gas (estimated)** | **~402,100** (2.4% of `EIP-7825`) | **~6,460,100** (38.5% of `EIP-7825`) |
