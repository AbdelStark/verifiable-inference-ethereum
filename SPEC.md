# Protocol and Contract Specification (`SPEC.md`)

Version: `0.2.0` (2026-10-27)
Pinned upstream SASH revision: `sg-ai-safety-hub/zk-inference-prover @ eb95bfa887299d1637550228d42086c27cbb2b3b`
Stwo as run by SASH: the vendored `nitroo/` copy in `zk-inference-prover @ eb95bfa`. It equals `AntChainOpenLabs/NitrooZK-stwo @ 41c2a05bdb1fc3e3762bc7804108e8bc269bb6fc` for the field, channel, Merkle and FRI primitives, and differs in `nitroo/crates/stwo/src/core/pcs/verifier.rs`, `nitroo/crates/stwo/src/core/verifier.rs`, `nitroo/crates/stwo/src/core/air/components.rs` and `nitroo/crates/constraint-framework`.

## 1. Proved statement and settlement digests

This section is the requirement the M2 segment guests and the M3 aggregator guest must meet. Neither guest exists yet. The contracts already implement their side of it (§1.3, §2).

### 1.1 Segment guests (M2)

Each segment guest runs SASH's unmodified `verify_composed` at `eb95bfa` on one segment and accepts only if `Verdict::accepts()` holds, which requires `embed_tok_ok && head_tok_ok` (`prover/src/compose.rs`). In addition, the guest MUST:

1. **Reject any token id `>= P`** (`P = 2^31 - 1`) in the `tok_in` or `tok_out` vector it checks. SASH maps ids to field elements with `M31::from_u32_unchecked` (`m31_u32`, `prover/src/common.rs`), so an id `>= P` would alias a canonical id in the token Horner sum.
2. **Output its segment claim** (§1.3) and, on `B0L0`, `tokenHash(tok_in)` of the exact vector its `embed_tok_ok` check used; on `head`, `tokenHash(tok_out)` of the exact vector its `head_tok_ok` check used.

### 1.2 Aggregator guest (M3) and settlement

The aggregator verifies all `N` segment proofs and commits `publicDigest` (§1.3), built from the segment claims and the two token hashes from §1.1. The BN254 wrapper's public values are exactly the 32 bytes of `publicDigest`.

`VerifiableInferenceSettlement` recomputes `publicDigest` from calldata (`modelId`, `segments`, `tokIn`, `tokOut`) and verifies the segment proof over it before any grid check. Any mutation of a segment claim, `tokIn` or `tokOut` therefore fails the segment-proof check: the SP1 proof with `Sp1SashSegmentVerifier`, or the dev tag with `DevTranscriptSegmentVerifier` (`chainid == 31337` only, no cryptographic binding).

### 1.3 Encodings

All hashes are `sha256`. `be16` / `be32` are big-endian `uint16` / `uint32`. sha256 is used rather than keccak256: keccak256 would save only an estimated ~2k gas per settlement.

- **Token ids:** `tokenHash(ids) = sha256(word(ids[0]) || ... || word(ids[k-1]))`, where `word(id)` is the 32-byte big-endian encoding of the `uint32` id (28 zero bytes, then `be32(id)`). This is Solidity's `sha256(abi.encodePacked(uint32[]))`. The same encoding gives `tokInHash = tokenHash(tokIn)`, `tokOutHash = tokenHash(tokOut)`, `promptHash = tokenHash(promptIds)` and `answerHash = tokenHash(answerIds)`.
- **Segment claim** (`130` bytes): `claim(i) = be16(segmentIndex) || preRoot || modelRoot || residualInRoot || residualOutRoot`.
- **`manifestRootsHash`** (`64 * N` bytes hashed): `sha256(preRoot[0] || modelRoot[0] || ... || preRoot[N-1] || modelRoot[N-1])`.
- **`publicDigest`** (`100 + 130 * N` bytes hashed): `sha256(modelId || be32(N) || claim(0) || ... || claim(N-1) || tokInHash || tokOutHash)`.
- **`runId`:** `sha256(modelId || publicDigest || promptHash || answerHash)`.
- **`modelId`** (fixture convention): `sha256("<model>@<revision>:<profile>")`.

### 1.4 TinyLlama values

Taken from an unpublished SASH run that M2 checks against the proof bundles: `TinyLlama/TinyLlama-1.1B-Chat-v1.0 @ fe8a4ea1ffedaf415f4da2f062534de366a451e6`, profile `y-attnout-w12`, `N = 23`, `S = 32` (`25` prompt + `8` answer tokens ending at EOS `2`), prompt `"Name the capital of France in one short sentence."`, response `"The capital of France is Paris."`. `contracts/src/sash/TinyLlamaFixture.sol` is generated from `crates/sash-evm-primitives/src/tinyllama_fixture.rs` (`make vectors`).

- `modelId` = `sha256("TinyLlama/TinyLlama-1.1B-Chat-v1.0@fe8a4ea1ffedaf415f4da2f062534de366a451e6:y-attnout-w12")` = `0x3def3a0832d3d82a82283200010c182132ac7d9eec681b772a95834f29e43c2a`
- `manifestRootsHash` = `0xfd8ecd8c79795b3a79e426d60a912d0a8a160188539c9b12dd3803b249c55b26`
- `tokInHash` = `0x3e683ee8603b85a4f5407e1af5f20a437ddadc9c098acc6aac357d533d3f32cb`
- `tokOutHash` = `0x44c6f339d896d66ea25d0ca6e15214f596ef5c701db79be94232be51d29b6b1e`
- `publicDigest` = `0x8c27cadf1bc21512bbb76eb63b23699b9af32d18b92d7cdf5a90a489b8000c6b`
- `promptHash` = `0xb4d5730f21772191057fb4600dc73dfdf0350fab6996c7b12d1d1ca7b83b9ec1`
- `answerHash` = `0xaed36ba9347820959ae88d81e8c45a733e404b199fb3c12a463e2301253934c8`
- `runId` = `0x0f614a4f82d3c3656dc9b15b897a5b07f621b312f5e37ae58c42b7391223e430`

## 2. Solidity contract interfaces and negative controls

1. **`SashModelRegistry.sol`:** Enrolls `ModelManifest { uint16 numSegments; uint16 seqLen; uint32 eosTokenId; bytes32 manifestRootsHash; bool active; }`, with `manifestRootsHash` as in §1.3.
2. **`SashGridVerifier.sol`:** Verifies the segment count, indices `0..N-1`, `manifestRootsHash`, the 22 residual-stream boundary root equalities (`segments[i].residualOutRoot == segments[i+1].residualInRoot`), `segments[N-1].residualOutRoot == bytes32(0)`, `tokIn.length == tokOut.length == seqLen`, the token window (`P + A - 1 == seqLen`), the `promptIds` prefix match, autoregressive loop closure (`tokIn[P + j] == tokOut[P - 1 + j]`), the `answerIds` match, and `tokOut[P + A - 2] == eosTokenId`. It does not range-check token ids: the guest statement covers them (§1.1).
3. **`VerifiableInferenceSettlement.sol`:** Checks `allowDevVerifier` against `segmentVerifier.verifierKind()`, checks replay protection (`settledRuns[runId]`), verifies `segmentVerifier.verifyAggregatedSegments(publicDigest, proofBytes)` with `publicDigest` as in §1.3, invokes `SashGridVerifier.verifyGrid`, records settlement, and emits `InferenceVerified`.

Negative controls (numbers 8, 9, 14 and 15 are retired and not reused):

1. forged `modelRoot`;
2. forged `preRoot`;
3. out-of-order `segmentIndex`;
4. broken residual boundary link;
5. non-zero `head` out-root;
6. mutated `tokIn`, submitted with the proof for the unmutated `publicDigest`;
7. mutated `tokOut`, submitted with the proof for the unmutated `publicDigest`;
10. mismatched `promptIds`;
11. broken teacher-forcing loop;
12. missing `EOS`;
13. mismatched `answerIds`;
16. wrong segment count;
17. corrupted proof bytes;
18. wrong `programVKey`;
19. `DevTranscriptSegmentVerifier` on a non-dev chain or with `allowDevVerifier == false`;
20. duplicate `runId` replay.

All of them MUST revert deterministically in both Rust and Foundry test suites.
