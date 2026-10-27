# Security Model and Assumption Boundary

`verifiable-inference-ethereum` is research and prototype infrastructure for verifying SASH LLM inference proofs on the Ethereum EVM. It has **not** undergone an external security audit and must not be used to guard high-value Mainnet funds without independent review.

## 1. What an accepted proof guarantees

When `VerifiableInferenceSettlement` accepts a run in `VerifierKind.ZKVM_GROTH16` mode (never `DEV_TRANSCRIPT_ONLY`, whose events prove nothing about SASH execution), and once the M2 and M3 guests exist and meet [SPEC.md](SPEC.md) §1:
1. Every segment $i \in \{0, \dots, 22\}$ committed the exact `(preRoot[i], modelRoot[i])` enrolled in `SashModelRegistry` for `modelId`.
2. Every segment passed the segment guest's rules ([SPEC.md](SPEC.md) §1.1): the verifier build of SASH's `verify_composed` with `Verdict::accepts()` (`log_blowup = 1`, `n_queries = 100`, `pow_bits = 0`), every token id `< P`, no unbound or lax gadget, sound dequant specs and no open requantize/dequantize seam. Upstream `accepts()` enforces only the first of these; the rest are guest rules.
3. All 22 inter-segment residual-stream Merkle roots chain without gap (`outRoot[L] == inRoot[L+1]`), and `head` carries no dangling output boundary (`outRoot[22] == 0`).
4. `tokIn` and `tokOut` are the exact vectors SASH's `embed_tok_ok` (segment 0) and `head_tok_ok` (`head`) checks accepted, with every id `< P`: their sha256 hashes are part of the `publicDigest` the segment proof attests ([SPEC.md](SPEC.md) §1, the statement the M2 and M3 guests must meet).
5. `tokIn[0..P-1]` equals `promptIds`, `tokIn[P+j] == tokOut[P-1+j]` closes every autoregressive step $j \in [0, A-2]$, `tokOut[P-1..P+A-2]` equals `answerIds`, and `tokOut[P+A-2] == eosTokenId`.

Two assumptions come with the guest:
- **`pre_root` comes from enrollment.** The guest does not re-derive the shape root (tree 0); it accepts the `pre_root` the statement names, and the manifest pins it. Its correctness rests on enrollment (§3).
- **The verifier build matches upstream.** The guest runs a verifier-only build of `verify_composed` at `eb95bfa`, differing from upstream only by the change classes [SPEC.md](SPEC.md) §1.1 lists, reproducible as a patch series and, from M2, checked by a differential test against upstream's verifier.

The manifest is also meant to pin each segment's `fixedHash` (the hash of its whole `Fixed` statement, [SPEC.md](SPEC.md) §1.1). That binding is planned and not yet enforced on-chain.

## 2. Cryptographic assumption shift across layers

| Layer | Primitive | Security basis | Post-quantum? | Trusted setup? |
|---|---|---|---|---|
| Inner SASH segment proof | Circle STARK (SASH's vendored `nitroo` Stwo, [SPEC.md](SPEC.md) header) + GKR5 over `M31`/`QM31` with `Blake2sM31Channel` | Collision/PRF resistance of `BLAKE2s-256`; ~100 bits conjectured FRI query soundness (`100 * 1 + 0`); `QM31` Schwartz-Zippel ($\le \text{deg}/2^{124}$) | Candidate yes | None |
| Outer zkVM wrapper (`ZKVM_GROTH16`, planned: M2 guests, M3 aggregator and wrap) | SP1 v6 `riscv64im` STARK, wrapped for BN254 (Groth16 or PLONK, chosen at M3 start) | BN254 pairing hardness (~100-bit classical taxonomy), integrity of that wrap's setup ceremony, SP1 v6 constraint soundness | **No** (Shor breaks BN254) | **Yes** (both candidate wraps rest on a setup ceremony) |
| On-chain grid & digest checks | `SashGridVerifier.sol` (grid checks, sha256 `publicDigest`) | Deterministic EVM execution by Ethereum consensus; `sha256` collision resistance for the `publicDigest` binding | Yes | None |

Effective classical soundness of the combined pipeline is bounded above by $\min(100, \lambda_{\text{zkVM}}, \lambda_{\text{BN254}}) \approx 100$ bits.

## 3. Non-guarantees

- **No zero-knowledge (privacy) guarantee:** SASH uses `LOG_BLOWUP = 1` without random trace blinding, and settlement exposes `promptIds` and `answerIds` in calldata. Do not submit confidential prompts or proprietary weights expecting cryptographic hiding.
- **Quantized relation only:** SASH proves its integer/lookup relation (`y-attnout-w12`), not IEEE-754 floating-point execution.
- **Manifest enrollment trust:** On-chain verification proves execution against the enrolled `(preRoot, modelRoot)` manifest. Enrollment must use the published calibration: the manifest that upstream `zk-prover model-roots --out` writes and that `zk-prover boundary-grid --verify --verify-calibration` consumes. Relying parties must verify offline that those roots were derived deterministically from the intended `model.safetensors` SHA-256 and that calibration. The segment guest does not re-derive the shape root, so each `preRoot` must be re-derived offline (`zk-prover expect-root --shape-only` or `zk-prover model-roots`) before trusting an enrolled manifest.

## 4. `DevTranscriptSegmentVerifier` isolation

`DevTranscriptSegmentVerifier.sol` exists solely so local Foundry workflows on chain `31337` (`forge test`, and `make anvil-demo` on a local anvil node) can exercise the full 23-segment TinyLlama settlement flow in milliseconds without waiting for GPU/zkVM proof compression. Three independent defenses prevent accidental production use:
1. `DevTranscriptSegmentVerifier` checks `block.chainid == 31337` in both its constructor and `verifyAggregatedSegments`.
2. `VerifiableInferenceSettlement` stores an immutable `allowDevVerifier` boolean; `DeployTestnet.s.sol` sets `allowDevVerifier = false`, reverting with `DevVerifierForbidden()` if a `DEV_TRANSCRIPT_ONLY` verifier is ever supplied.
3. Every `InferenceVerified` event emits `VerifierKind` explicitly so off-chain indexers distinguish dev-mode runs (`0`) from cryptographic proofs (`1`).
