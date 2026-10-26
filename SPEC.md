# Protocol and Contract Specification (`SPEC.md`)

Version: `0.1.0` (2026-10-26)
Pinned upstream SASH revision: `sg-ai-safety-hub/zk-inference-prover @ eb95bfa887299d1637550228d42086c27cbb2b3b`
Pinned upstream Circle STARK core: `AntChainOpenLabs/NitrooZK-stwo @ 41c2a05bdb1fc3e3762bc7804108e8bc269bb6fc`

## 1. Field tower (`M31`, `CM31`, `QM31`)

1. **`M31`:** Prime modulus $P = 2^{31} - 1 = 2,147,483,647$ (`0x7fffffff`). Canonical elements satisfy $0 \le x < P$. Calldata decoders MUST revert with `NonCanonicalM31(uint32 value)` on any limb $\ge P$.
2. **`CM31`:** $M_{31}[i]/(i^2 + 1)$, pair `(a0, a1)` with $(a_0 + a_1 i)(b_0 + b_1 i) = (a_0 b_0 - a_1 b_1) + (a_0 b_1 + a_1 b_0)i \pmod P$.
3. **`QM31` (`SecureField`):** $CM_{31}[u]/(u^2 - (2 + i))$, quadruple `(a0, a1, b0, b1)` representing $(a_0 + a_1 i) + (b_0 + b_1 i)u$. Multiplication uses $u^2 = 2 + i$: for $x = (x_0, x_1) \in CM_{31}$, $(2 + i)x = (2 x_0 - x_1, x_0 + 2 x_1) \pmod P$. Little-endian byte encoding writes `le32(a0) || le32(a1) || le32(b0) || le32(b1)` (16 bytes).

## 2. `Blake2sM31Hasher` and `Blake2sM31Channel`

SASH instantiates `Blake2sM31Channel` and `Blake2sM31MerkleChannel` (`IS_M31_OUTPUT = true`), **not** unreduced `Blake2sChannel`:

1. **`blake2s256(data) -> [u8; 32]`:** Unkeyed RFC 7693 `BLAKE2s-256` (`IV[0] ^ 0x01010020 = 0x6b08e647`, 10 rounds, 64-byte blocks).
2. **`reduce_to_m31(raw32) -> [u8; 32]`:** Unpack `raw32` into eight little-endian `u32` words $w_0 \dots w_7$, replace each $w_j \gets w_j \bmod P$, and pack back to 32 little-endian bytes.
3. **Merkle node hashing (`Blake2sMerkleHasherGeneric<true>::hash_node`):**
   - Leaf-only node: `reduce_to_m31(blake2s256(le32(col_0) || ... || le32(col_{k-1})))`.
   - Internal binary node without column values: `reduce_to_m31(blake2s256(left_hash32 || right_hash32))`.
   - Mixed internal node with column values: `reduce_to_m31(blake2s256(left_hash32 || right_hash32 || le32(col_0) || ...))`.
4. **Channel mixing and drawing:**
   - Initial state: `digest = [0u8; 32]`, `n_draws = 0u32`.
   - `mix_root(root32)`: `digest = reduce_to_m31(blake2s256(digest || root32))`; `n_draws = 0`.
   - `mix_felts(felts)`: `digest = reduce_to_m31(blake2s256(digest || le_qm31_bytes(felts)))`; `n_draws = 0`.
   - `draw_base_felts() -> [M31; 8]`: Loop with `msg = digest (32 B) || le32(n_draws) (4 B) || 0x00 (1 B)`, `n_draws += 1`, `raw = blake2s256(msg)` (unreduced!). Unpack eight LE `u32` words; if all 8 satisfy $w_j < 2P = 4,294,967,294$ (`0xfffffffe`), return `[w_j % P]`; else retry.
   - `draw_secure_felts(n) -> Vec<QM31>`: Draw $\lceil 4n / 8 \rceil$ batches of 8 base felts, truncate to $4n$, and group into `QM31` quadruples. Note that `draw_secure_felts(2)` consumes **1** batch of 8 `M31`s.

## 3. SASH Fiat-Shamir `w_residual_stream` and Horner token bindings

For a SASH segment with fixed security parameters `(pow_bits = 0, log_blowup = 1, n_queries = 100, log_last_layer = 0, line_fold_step = 1, lifting_log_size = 0)`:

1. `ch = Blake2sM31Channel::default()`
2. `ch.mix_felts([QM31(0, 1, 100, 0), QM31(1, 0, 0, 0)])`
3. `ch.mix_root(pre_root)`; `ch.mix_root(model_root)`; `ch.mix_root(trace_root)`
4. Repeat `relation_draws` times: `ch.draw_secure_felts(2)` (`29` on `B0L0`, `24` on `head` for TinyLlama-1.1B `y-attnout-w12`).
5. `f = ch.draw_secure_felts(5)`; `w_residual_stream = f[3]`.
6. Token polynomial bindings:
   - Segment `0` (`B0L0`, ascending): $\text{embed\_tok} = \sum_{k=0}^{S-1} \text{tok\_in}[k] \cdot w^{S-1-k} \in QM_{31}$.
   - Segment `N-1` (`head`, descending): $\text{head\_tok} = \sum_{k=0}^{S-1} \text{tok\_out}[k] \cdot w^{k} \in QM_{31}$.

### Primary-source test vectors (`TinyLlama-1.1B-Chat-v1.0`, `y-attnout-w12`, `S = 32`)

- **`B0L0` (`segmentIndex = 0`, `relationDraws = 29`):**
  - `preRoot`: `0x7da3534f535fe416c9d9aa0e8f765e3da5ac8c772d0ef35e6f303103c099466f`
  - `modelRoot`: `0xa45136572e13045f3415eb12e82c437a0a83f276fb2699765b32f70c00c3de34`
  - `traceRoot`: `0xd937eb517f80d61ed39333010dd9f12fc64a9a07d2706675d75dda2715dc6e57`
  - `w_residual_stream`: `QM31(1259081484, 630063634, 1062868201, 1393098785)`
  - `embed_tok`: `QM31(1205748927, 962712399, 1890095260, 1449788258)`
- **`head` (`segmentIndex = 22`, `relationDraws = 24`):**
  - `preRoot`: `0x3119373feaf0611ffaa7275ba4a6d031e33eeb3e2710ff41b1a9582431a8e73f`
  - `modelRoot`: `0xe26a2e1be19e2b6d3f1ea17dc311e16c8fb1b523633e4e6a754720486220ce12`
  - `traceRoot`: `0x8eeda8657770717a942bd218b852681bc76de5181169332bfa62586264b15a14`
  - `w_residual_stream`: `QM31(1297757949, 994229370, 1478950871, 2099916732)`
  - `head_tok`: `QM31(754377417, 2109312062, 1514847, 2019687017)`

## 4. `SASHBIN1` compact binary segment format (`crates/sash-repack`)

To avoid 84.61 MB JSON DOM allocations inside a 32-bit RISC-V zkVM guest, `sash-repack` encodes each validated segment into a deterministic little-endian byte stream:

```text
Offset  Size    Field
0       8       Magic bytes: b"SASHBIN1"
8       2       segment_index (u16 LE)
10      2       relation_draws (u16 LE)
12      32      pre_root ([u8; 32])
44      32      model_root ([u8; 32])
76      32      trace_root ([u8; 32])
108     32      residual_in_root ([u8; 32])
140     32      residual_out_root ([u8; 32], all-zero on head)
172     16      token_horner_claim (4 x u32 LE, M31 canonical)
188     4       fixed_len (u32 LE) + fixed_bytes
...     4       claims_no_gkr_len (u32 LE) + claims_no_gkr_bytes
...     4       gkr_len (u32 LE) + raw gkr Vec<u8> bytes
...     4       stark_bin_len (u32 LE) + packed binary StarkProof
```

## 5. Solidity contract interfaces and 20 negative controls

1. **`SashModelRegistry.sol`:** Enrolls `ModelManifest { uint16 numSegments; uint16 seqLen; uint32 eosTokenId; uint16 b0RelDraws; uint16 headRelDraws; bytes32 manifestRootsHash; bool active; }` where `manifestRootsHash = keccak256(abi.encodePacked(preRoot[0], modelRoot[0], ..., preRoot[N-1], modelRoot[N-1]))`.
2. **`SashGridVerifier.sol`:** Verifies segment count, indices `0..N-1`, `manifestRootsHash`, 22 residual-stream boundary root equalities (`segments[i].residualOutRoot == segments[i+1].residualInRoot`), `segments[N-1].residualOutRoot == bytes32(0)`, `relationDraws` pin on `B0L0` and `head`, `Blake2sM31Channel.deriveWResidualStream` + `QM31.hornerAsc` / `hornerDesc` equalities, canonical `< P` checks on all tokens and `QM31` limbs, `promptIds` prefix match, autoregressive loop closure (`tokIn[P + j] == tokOut[P - 1 + j]`), `answerIds` match, and `tokOut[P + A - 2] == eosTokenId`.
3. **`VerifiableInferenceSettlement.sol`:** Checks replay protection (`settledRuns[runId]`), checks `allowDevVerifier` against `segmentVerifier.verifierKind()`, verifies `segmentVerifier.verifyAggregatedSegments(publicDigest, proofBytes)` where `publicDigest = keccak256(abi.encode(modelId, segments))`, invokes `SashGridVerifier.verifyGrid`, records settlement, and emits `InferenceVerified`.

All 20 negative controls (forged `modelRoot`, forged `preRoot`, out-of-order `segmentIndex`, broken residual boundary link, non-zero `head` out-root, mutated `tokIn` breaking `embed_tok`, mutated `tokOut` breaking `head_tok`, mutated `traceRoot` shifting `w_residual_stream`, wrong `relationDraws`, mismatched `promptIds`, broken teacher-forcing loop, missing `EOS`, mismatched `answerIds`, non-canonical `M31` in claim, non-canonical token ID, empty/wrong segment count, corrupted `publicDigest` / proof bytes, wrong `programVKey`, `DevTranscriptSegmentVerifier` on non-dev chain or `allowDevVerifier == false`, and duplicate `runId` replay) MUST revert deterministically in both Rust and Foundry test suites.
