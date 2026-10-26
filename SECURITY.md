# Security Model and Assumption Boundary

`verifiable-inference-ethereum` is research and prototype infrastructure for verifying SASH LLM inference proofs on the Ethereum EVM. It has **not** undergone an external security audit and must not be used to guard high-value Mainnet funds without independent review.

## 1. What an accepted proof guarantees

When `VerifiableInferenceSettlement` accepts a run in `VerifierKind.ZKVM_GROTH16` mode:
1. Every segment $i \in \{0, \dots, 22\}$ committed the exact `(preRoot[i], modelRoot[i])` enrolled in `SashModelRegistry` for `modelId`.
2. Every segment passed SASH's `verify_composed` (`log_blowup = 1`, `n_queries = 100`, `pow_bits = 0`) with zero unsound skips or open requantize/dequantize seams.
3. All 22 inter-segment residual-stream Merkle roots chain without gap (`outRoot[L] == inRoot[L+1]`), and `head` carries no dangling output boundary (`outRoot[22] == 0`).
4. Replaying `Blake2sM31Channel` on-chain from `(preRoot, modelRoot, traceRoot)` yields `w_residual_stream` challenges under which `hornerAsc(tokIn, w) == embedTok` on segment 0 and `hornerDesc(tokOut, w) == headTok` on `head`.
5. `tokIn[0..P-1]` equals `promptIds`, `tokIn[P+j] == tokOut[P-1+j]` closes every autoregressive step $j \in [0, A-2]$, `tokOut[P-1..P+A-2]` equals `answerIds`, and `tokOut[P+A-2] == eosTokenId`.

## 2. Cryptographic assumption shift across layers

| Layer | Primitive | Security basis | Post-quantum? | Trusted setup? |
|---|---|---|---|---|
| Inner SASH segment proof | Circle STARK (`NitrooZK-stwo`) + GKR5 over `M31`/`QM31` with `Blake2sM31Channel` | Collision/PRF resistance of `BLAKE2s-256`; ~100 bits conjectured FRI query soundness (`100 * 1 + 0`); `QM31` Schwartz-Zippel ($\le \text{deg}/2^{124}$) | Candidate yes | None |
| Outer zkVM wrapper (`ZKVM_GROTH16`) | SP1 / RISC Zero RISC-V STARK compressed to BN254 Groth16 | BN254 pairing hardness (~100-bit classical taxonomy), Groth16 SRS ceremony toxic-waste destruction, RISC-V zkVM constraint soundness | **No** (Shor breaks BN254) | **Yes** (Groth16 SRS) |
| On-chain grid & Horner verifier | `SashGridVerifier.sol` + `Blake2sM31Channel.sol` | Deterministic EVM execution by Ethereum consensus | Yes (unconditional given EVM consensus) | None |

Effective classical soundness of the combined pipeline is bounded above by $\min(100, \lambda_{\text{zkVM}}, \lambda_{\text{BN254}}) \approx 100$ bits.

## 3. Non-guarantees

- **No zero-knowledge (privacy) guarantee:** SASH uses `LOG_BLOWUP = 1` without random trace blinding, and settlement exposes `promptIds` and `answerIds` in calldata. Do not submit confidential prompts or proprietary weights expecting cryptographic hiding.
- **Quantized relation only:** SASH proves its integer/lookup relation (`y-attnout-w12`), not IEEE-754 floating-point execution.
- **Manifest enrollment trust:** On-chain verification proves execution against the enrolled `(preRoot, modelRoot)` manifest. Relying parties must verify offline that those roots were derived deterministically from the intended `model.safetensors` SHA-256 and calibration set.

## 4. `DevTranscriptSegmentVerifier` isolation

`DevTranscriptSegmentVerifier.sol` exists solely so local Foundry (`forge test`) and Anvil (`chainid == 31337`) workflows can exercise the full 23-segment TinyLlama settlement flow in milliseconds without waiting for GPU/zkVM proof compression. Three independent defenses prevent accidental production use:
1. `DevTranscriptSegmentVerifier` checks `block.chainid == 31337` in both its constructor and `verifyAggregatedSegments`.
2. `VerifiableInferenceSettlement` stores an immutable `allowDevVerifier` boolean; `DeployTestnet.s.sol` sets `allowDevVerifier = false`, reverting with `DevVerifierForbidden()` if a `DEV_TRANSCRIPT_ONLY` verifier is ever supplied.
3. Every `InferenceVerified` event emits `VerifierKind` explicitly so off-chain indexers distinguish dev-mode runs (`0`) from cryptographic proofs (`1`).
