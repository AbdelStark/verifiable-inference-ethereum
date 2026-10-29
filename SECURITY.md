# Security Model and Assumption Boundary

`verifiable-inference-ethereum` is research and prototype infrastructure for verifying SASH LLM inference proofs on the Ethereum EVM. It has **not** undergone an external security audit and must not be used to guard high-value Mainnet funds without independent review.

## 1. What an accepted proof guarantees

When `VerifiableInferenceSettlement` accepts a run in `VerifierKind.ZKVM_GROTH16` mode (never `DEV_TRANSCRIPT_ONLY`, whose events prove nothing about SASH execution), and once the M2 and M3 guests exist and meet [SPEC.md](SPEC.md) §1:
1. Every segment $i \in \{0, \dots, 22\}$ committed the exact `(preRoot[i], modelRoot[i], fixedHash[i])` enrolled in `SashModelRegistry` for `modelId`, where `fixedHash[i]` is the hash of segment $i$'s whole `Fixed` statement ([SPEC.md](SPEC.md) §1.1): its params, `sizes`, `layer`, `block` and `model`, which the verifier build checks only in part or not at all.
2. Every segment passed the segment guest's rules ([SPEC.md](SPEC.md) §1.1): the verifier build of SASH's `verify_composed` with `Verdict::accepts()` (`log_blowup = 1`, `n_queries = 100`, `pow_bits = 0`), every token id `< P`, no unbound or lax gadget, sound dequant specs and no open requantize/dequantize seam. Upstream `accepts()` enforces only the first of these; the rest are guest rules.
3. All 22 inter-segment residual-stream Merkle roots chain without gap (`outRoot[L] == inRoot[L+1]`), and `head` carries no dangling output boundary (`outRoot[22] == 0`).
4. `tokIn` and `tokOut` are the ids the embedding (segment 0) and the argmax (`head`) committed in their proofs, with every id `< P`: SASH with patch 0006 mixes the published vectors into the Fiat-Shamir transcript before drawing the challenge `embed_tok_ok` and `head_tok_ok` check them under (§5), and their sha256 hashes are part of the `publicDigest` the segment proof attests ([SPEC.md](SPEC.md) §1, the statement the M2 and M3 guests must meet).
5. `tokIn[0..P-1]` equals `promptIds`, `tokIn[P+j] == tokOut[P-1+j]` closes every autoregressive step $j \in [0, A-2]$, `tokOut[P-1..P+A-2]` equals `answerIds`, and `tokOut[P+A-2] == eosTokenId`.

Two assumptions come with the guest:
- **`pre_root` comes from enrollment.** The guest does not re-derive the shape root (tree 0); it accepts the `pre_root` the statement names, and the manifest pins it. Its correctness rests on enrollment (§3).
- **The verifier build matches upstream plus patch 0006.** The guest runs a verifier-only build of `verify_composed` at `eb95bfa`, differing from upstream only by the change classes [SPEC.md](SPEC.md) §1.1 lists, reproducible as a patch series and, from M2, checked by a differential test against upstream's verifier with patch 0006 alone. Patch 0006 is the one class that changes behaviour, on the prover's side too (§5).

The manifest pins each segment's `fixedHash`, and `SashGridVerifier` enforces it on-chain: `fixedHash` is in each segment claim and in `manifestRootsHash` ([SPEC.md](SPEC.md) §1.3), so a proof against another `Fixed` with the same two roots fails with `ManifestRootMismatch`. The guest compares it with no pin; it commits the hash it computed.

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
- **Manifest enrollment trust:** On-chain verification proves execution against the enrolled `(preRoot, modelRoot, fixedHash)` manifest. Enrollment must use the published calibration: the manifest that upstream `zk-prover model-roots --out` writes and that `zk-prover boundary-grid --verify --verify-calibration` consumes. Relying parties must verify offline that those roots were derived deterministically from the intended `model.safetensors` SHA-256 and that calibration. The segment guest does not re-derive the shape root, so each `preRoot` must be re-derived offline (`zk-prover expect-root --shape-only` or `zk-prover model-roots`) before trusting an enrolled manifest. Likewise, relying parties must recompute each segment's `fixedHash` from its `Fixed` ([SPEC.md](SPEC.md) §1.1) before trusting an enrolled manifest.

## 4. `DevTranscriptSegmentVerifier` isolation

`DevTranscriptSegmentVerifier.sol` exists solely so local Foundry workflows on chain `31337` (`forge test`, and `make anvil-demo` on a local anvil node) can exercise the full 23-segment TinyLlama settlement flow in milliseconds without waiting for GPU/zkVM proof compression. Three independent defenses prevent accidental production use:
1. `DevTranscriptSegmentVerifier` checks `block.chainid == 31337` in both its constructor and `verifyAggregatedSegments`.
2. `VerifiableInferenceSettlement` stores an immutable `allowDevVerifier` boolean; `DeployTestnet.s.sol` sets `allowDevVerifier = false`, reverting with `DevVerifierForbidden()` if a `DEV_TRANSCRIPT_ONLY` verifier is ever supplied.
3. Every `InferenceVerified` event emits `VerifierKind` explicitly so off-chain indexers distinguish dev-mode runs (`0`) from cryptographic proofs (`1`).

## 5. Token vectors and the Fiat-Shamir transcript (patch 0006)

**What was wrong.** Upstream SASH `eb95bfa` never mixes the published token vectors `Fixed.tok_in` and `Fixed.tok_out` into its Fiat-Shamir channel. `embed_tok_ok` and `head_tok_ok` only check that their Horner sums under the challenge `w_residual_stream` equal the claims `embed_tok` and `head_tok`, and the ids committed in the trace are fixed before `w`, but the published vectors are not. Anyone holding a valid bundle can replay the transcript to `w`, change any token positions, and solve one QM31 equation (four linear equations over M31) for four slack positions. Every id stays `< P`, `fixedHash` and the claims do not change, and the verifier accepts. Reproduced on the real TinyLlama `B0L0` (`tok_in[25]`, slack `1..4`) and `head` (`tok_out[24]`, slack `0..3`) bundles. Before the fix this let a prover:
- settle a response the model never produced, by changing answer token `tokOut[24]` together with prompt token `tokIn[25]` (loop closure holds) and placing the slack in `tokOut[0..P-2]`, which the grid never reads;
- settle one honest run again once per Horner-preserving edit of `tokOut[0..P-2]`, each with the same `promptIds` and `answerIds` but another `tokOutHash`, `publicDigest` and `runId`.

Every other `Fixed` field is pinned by `fixedHash`, and every `Claims` field is mixed (`Claims::flat`): the token vectors were the only free input.

**What is now bound.** `sp1/patches/0006-bind-token-vectors-into-transcript.patch` mixes `len(tok_in) || tok_in || len(tok_out) || tok_out`, the exact `u32` ids, after tree 2 and before the first relation draw, in the prover and in `verify_composed` alike ([SPEC.md](SPEC.md) §1.1). `w_residual_stream` is then drawn after the vectors are fixed, so a second vector with the same Horner image is a Schwartz-Zippel collision rather than a choice, and an edited vector draws another `w`. The guest-level control G9 ([SPEC.md](SPEC.md) §2.1) forges each vector under the `w` its bundle was proved with, on the synthetic corpus' `embed` and `head_argmax` bundles in CI and on the real `B0L0` and `head` in `make sp1-execute`, natively and in the SP1 executor, and requires a rejection; a verifier without the patch accepts those forgeries.

**The fork from upstream.** The patch changes the prover's transcript as well as the verifier's. A segment that publishes no token vector (21 of TinyLlama's 23, and every synthetic corpus bundle but `embed` and `head_argmax`) mixes nothing and proves byte for byte as upstream does. `B0L0` and `head` must be proved by a SASH prover with patch 0006: their upstream proofs reject in the guest, and their patched proofs reject under upstream's verifier. The TinyLlama bundles are proved this way ([SPEC.md](SPEC.md) §1.4); on them G9, and the same forgery solved under upstream's `w`, both reject natively and in the SP1 executor ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §7). The project follows upstream's fix when it ships.
