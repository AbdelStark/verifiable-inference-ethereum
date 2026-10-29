# Architecture Overview

## 1. End-to-end verification dataflow

```text
+-------------------------------------------------------------------------+
| GPU Prover Host (SASH zk-inference-prover @ eb95bfa + patch 0006,       |
|   which mixes the published token vectors into the transcript)          |
|   Model: TinyLlama-1.1B-Chat-v1.0 (fe8a4ea, profile y-attnout-w12)      |
|   Outputs 23 segment bundles: B0L0 .. B0L21 + head (761 MB JSON in the  |
|   published run; all 23 verify in the M2 guest)                         |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| Off-Chain Proof Compression (M2 segment guest present; M3 planned)      |
|   - M2: SP1 v6 riscv64im guest per segment runs a verifier-only build   |
|     of verify_composed (+ patch 0006); rejects token ids >= P;          |
|     commits fixedHash                                                   |
|   - M3: aggregator verifies 23 proofs, commits publicDigest             |
|   - M3: SP1 v6 BN254 wrap (Groth16 or PLONK, chosen at M3 start)        |
+------------------------------------+------------------------------------+
                                     |
           Single Ethereum Transaction (< 600k gas: M3 exit gate)
                                     |
                                     v
+-------------------------------------------------------------------------+
| Ethereum EVM (local anvil, chain 31337 -> Hoodi 560048 (M4))            |
|                                                                         |
|  [VerifiableInferenceSettlement.sol]                                    |
|     |                                                                   |
|     +--> 1. [SashModelRegistry.sol]                                     |
|     |         Loads enrolled ModelManifest & manifestRootsHash          |
|     |                                                                   |
|     +--> 2. [ISashSegmentVerifier.sol]                                  |
|     |         - Sp1SashSegmentVerifier (SP1 BN254 wrap; proofs from M3) |
|     |         - DevTranscriptSegmentVerifier (chainid==31337 dev only)  |
|     |                                                                   |
|     +--> 3. [SashGridVerifier.sol]                                      |
|               - Verifies 23 (preRoot, modelRoot, fixedHash) == manifest |
|               - Verifies 22 residual links: outRoot[L] == inRoot[L+1]   |
|               - Hashes tokIn & tokOut into publicDigest (sha256)        |
|               - Verifies prompt prefix, autoregressive loop, and EOS    |
+-------------------------------------------------------------------------+
```

## 2. Why `SashGridVerifier.sol` runs natively on the EVM instead of moving everything off-chain

A naive zkVM wrapper would collapse the entire 23-segment LLM run into a single opaque boolean (`true`) inside the RISC-V guest. While slightly cheaper in calldata (an estimated ~4.3 KiB saved: 23 claims × 192 B of ABI calldata), opaque aggregation hides protocol structure from on-chain contracts and concentrates 100% of binding logic inside off-chain guest code.

By keeping **manifest admission, 22-layer residual Merkle root chaining, the sha256 `publicDigest` over the segment claims and both token vectors, prompt/response slicing, teacher-forcing loop closure, and EOS checking** native in Solidity (~76k gas measured: `getManifest`, `verifyGrid` and `computePublicDigest`, [EVM-GAS-MODEL.md](EVM-GAS-MODEL.md) §4):
1. Smart contracts inspect and bind per-layer `modelRoot`, `preRoot` and `fixedHash` commitments directly on-chain.
2. Any bug in token indexing, prompt prefix slicing, or boundary chaining is prevented by small, auditable, unconditional EVM code.
3. The segment proof interface (`ISashSegmentVerifier`) stays modular: changing the proof system behind it, such as the BN254 wrap that M3 chooses for `Sp1SashSegmentVerifier`, requires **zero** changes to `SashModelRegistry`, `SashGridVerifier`, or downstream consumer contracts.
