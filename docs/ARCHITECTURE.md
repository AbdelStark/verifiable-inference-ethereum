# Architecture Overview

## 1. End-to-end verification dataflow

```text
+-------------------------------------------------------------------------+
| GPU Prover Host (SASH zk-inference-prover @ eb95bfa)                    |
|   Model: TinyLlama-1.1B-Chat-v1.0 (fe8a4ea, profile y-attnout-w12)      |
|   Outputs 23 segment bundles: B0L0 .. B0L21 + head (761 MB JSON)        |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| Host Binary Repacker (crates/sash-repack)                               |
|   - Validates (log_blowup=1, n_queries=100, pow_bits=0)                 |
|   - Validates enrolled (pre_root, model_root) & 22 residual links       |
|   - Repacks JSON -> SASHBIN1 binary (296.5 MB total, <= 34.5 MB/seg)    |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| Off-Chain Proof Compression (RISC-V zkVM Guest -> BN254 Groth16)        |
|   - Runs SASH verify_composed (no-cuda, pre-enrolled shape roots)       |
|   - Emits 23 SegmentClaim structs; aggregates into publicDigest         |
|   - Wraps into BN254 Groth16 proof (~260-384 bytes)                     |
+------------------------------------+------------------------------------+
                                     |
                      Single Ethereum Transaction (< 600k gas)
                                     |
                                     v
+-------------------------------------------------------------------------+
| Ethereum EVM (Foundry -> Anvil -> Sepolia 11155111 / Hoodi 560048)      |
|                                                                         |
|  [VerifiableInferenceSettlement.sol]                                    |
|     |                                                                   |
|     +--> 1. [SashModelRegistry.sol]                                     |
|     |         Loads enrolled ModelManifest & manifestRootsHash          |
|     |                                                                   |
|     +--> 2. [ISashSegmentVerifier.sol]                                  |
|     |         - Sp1SashSegmentVerifier (BN254 Groth16 via 0x08)         |
|     |         - DevTranscriptSegmentVerifier (chainid==31337 dev only)  |
|     |                                                                   |
|     +--> 3. [SashGridVerifier.sol]                                      |
|               - Verifies 23 (preRoot, modelRoot) pairs == manifest      |
|               - Verifies 22 residual links: outRoot[L] == inRoot[L+1]   |
|               - Replays [Blake2sM31Channel.sol] on B0L0 & head          |
|               - Verifies [QM31.sol] hornerAsc(tokIn) & hornerDesc       |
|               - Verifies prompt prefix, autoregressive loop, and EOS    |
+-------------------------------------------------------------------------+
```

## 2. Why `SashGridVerifier.sol` runs natively on the EVM instead of moving everything off-chain

A naive zkVM wrapper would collapse the entire 23-segment LLM run into a single opaque boolean (`true`) inside the RISC-V guest. While slightly cheaper in calldata (~3.5 KiB saved), opaque aggregation hides protocol structure from on-chain contracts and concentrates 100% of binding logic inside off-chain guest code.

By keeping **manifest admission, 22-layer residual Merkle root chaining, `Blake2sM31Channel` `w_residual_stream` derivation, `QM31` Horner evaluation, prompt/response slicing, teacher-forcing loop closure, and EOS checking** native in Solidity (~185k gas total):
1. Smart contracts inspect and bind per-layer `modelRoot` and `preRoot` commitments directly on-chain.
2. Any bug in token indexing, prompt prefix slicing, or boundary chaining is prevented by small, auditable, unconditional EVM code.
3. The segment proof interface (`ISashSegmentVerifier`) becomes completely modular: swapping BN254 Groth16 (`Sp1SashSegmentVerifier`) for RISC Zero or a future native recursive Stwo Circle STARK (`CircleStarkVerifier`) requires **zero** changes to `SashModelRegistry`, `SashGridVerifier`, or downstream consumer contracts.
