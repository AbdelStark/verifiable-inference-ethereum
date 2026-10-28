# Protocol and Contract Specification (`SPEC.md`)

Version: `0.3.0` (2026-10-27)
Pinned upstream SASH revision: `sg-ai-safety-hub/zk-inference-prover @ eb95bfa887299d1637550228d42086c27cbb2b3b`
Stwo as run by SASH: the vendored `nitroo/` copy in `zk-inference-prover @ eb95bfa`. It equals `AntChainOpenLabs/NitrooZK-stwo @ 41c2a05bdb1fc3e3762bc7804108e8bc269bb6fc` for the field, channel, Merkle and FRI primitives, and differs in `nitroo/crates/stwo/src/core/pcs/verifier.rs`, `nitroo/crates/stwo/src/core/verifier.rs`, `nitroo/crates/stwo/src/core/air/components.rs` and `nitroo/crates/constraint-framework`.
Files patched against `eb95bfa` for the segment guests' verifier build (§1.1), by `sp1/patches/`: `nitroo/crates/stwo/build.rs`, `nitroo/crates/constraint-framework/src/lib.rs`, `nitroo/crates/stwo/src/core/mod.rs`, `nitroo/crates/stwo/src/core/lookups/{mod,gkr_verifier,sumcheck,utils}.rs` (new), `nitroo/crates/stwo/src/prover/lookups/{gkr_verifier,sumcheck,utils}.rs`, `prover/src/gkr_key_tile.rs`, `prover/src/gkrlink.rs`, and for the `zk-prover` cfg-split `prover/Cargo.toml`, `prover/build.rs`, `prover/src/bf16.rs`, `prover/src/boundary.rs`, `prover/src/common.rs`, `prover/src/compose.rs`, `prover/src/encoding.rs`, `prover/src/gadgets/argmax/mod.rs`, `prover/src/gadgets/attention/air.rs`, `prover/src/gadgets/attention/mod.rs`, `prover/src/gadgets/attention/trace.rs`, `prover/src/gadgets/attention/witness.rs`, `prover/src/gadgets/attn_out/air.rs`, `prover/src/gadgets/attn_out/claims.rs`, `prover/src/gadgets/attn_out/mod.rs`, `prover/src/gadgets/attn_out/trace.rs`, `prover/src/gadgets/dequant/claims.rs`, `prover/src/gadgets/dequant/mod.rs`, `prover/src/gadgets/dequant/trace.rs`, `prover/src/gadgets/embed/mod.rs`, `prover/src/gadgets/freivalds/claims.rs`, `prover/src/gadgets/freivalds/mod.rs`, `prover/src/gadgets/freivalds/trace.rs`, `prover/src/gadgets/requant/mod.rs`, `prover/src/gadgets/requant/tables.rs`, `prover/src/gadgets/residual_stream/mod.rs`, `prover/src/gadgets/residual_stream/witness.rs`, `prover/src/gadgets/rope/mod.rs`, `prover/src/gadgets/rope/witness.rs`, `prover/src/layer.rs`, `prover/src/lib.rs`, `prover/src/relations.rs`, `prover/src/util.rs`, and for the `verify_composed` split `prover/src/compose.rs`.

## 1. Proved statement and settlement digests

This section is the requirement the M2 segment guests and the M3 aggregator guest must meet. Neither guest exists yet. The contracts implement §1.3 and §2 as written. Binding each segment's `fixedHash` (§1.1) into the segment claim, `manifestRootsHash` and `publicDigest` is a planned M2 change to §1.3, not implemented yet.

### 1.1 Segment guests (M2)

Each segment guest verifies one SASH segment (`<tag>.fixed.json`, `<tag>.claims.json`, `<tag>.proof.json`) with a verifier-only build of `verify_composed` (`prover/src/compose.rs`) at `eb95bfa`, and commits the segment's public values.

**Verifier build.** Upstream `verify_composed` cannot run in an SP1 guest: it calls `std::time::Instant::now`, its crate enables stwo's `prover` feature (which needs `array_chunks`, removed from Rust in 1.90), and nitroo's stwo `build.rs` runs CUDA cmake on every non-macOS host. The verifier build MAY differ from upstream only by:
- stwo's `build.rs` gated on the `prover` feature;
- constraint-framework's `mod cuda_domain` and its re-export gated on `prover`;
- the GKR verifier (`gkr_verifier.rs`, the verifier half of `sumcheck.rs`, `utils.rs`) moved from `stwo::prover::lookups` to `stwo::core::lookups`, with re-exports and import fixes;
- `zk-prover` cfg-split under a default-on `prover` feature;
- `Instant::now` and `Verdict.verify_secs` cfg-gated out of the verifier build;
- `shape_root_check`, `shape_root_from_params` and their `catch_unwind` cfg-gated out of the verifier build, with `verify_composed` binding upstream's no-calibration value `(None, None, 0.0, None)` in their place;
- `verify_composed` split into byte decoding and `verify_composed_parsed`, a verify over the parsed `(Fixed, Claims, Proof)` and the `Plan` of `Fixed.params`, the byte entry building that `Plan` and delegating to it unchanged. The segment check builds the `Plan` once, for rules 4 and 5, and passes it.

The difference MUST be reproducible as a reviewed patch series against `eb95bfa`, and the header lists every patched file. Any other change needs an edit to this list in the change that makes it.

**Guest input.** The guest reads three buffers with `sp1_zkvm::io::read_vec`, in this order:
- `fixed`: the bytes of `<tag>.fixed.json`, decoded as `serde_json`;
- `claims`: the bytes of `<tag>.claims.json`, decoded as `serde_json`;
- `proof`: the `StarkProof` the host parses from `<tag>.proof.json` and re-encodes as `bincode` 1.3.3 (`bincode::serialize`: fixed-width little-endian integers, `u64` lengths).

The guest decodes the buffers itself and calls the parsed entry of `verify_composed`. A `fixed` that does not decode is `Malformed`, and a `claims` or `proof` that does not decode rejects at rule 6. Whatever the wire format, `fixedHash` hashes the re-serialized parsed `Fixed`. The re-encoding is outside the trusted base: the guest verifies whatever proof it decodes.

These formats come from measurements (`docs/EVM-GAS-MODEL.md` §6.2). Decoding `sound_aligned`'s `StarkProof` costs 29.6 cycles per JSON byte, 9.6 per `bincode` byte and 11.8 per `postcard` byte. Re-encoded, the proof is 0.386 of the JSON size in `bincode` and 0.445 in `postcard`. That is 699M cycles as JSON, 87M as `bincode` and 124M as `postcard`. Over the TinyLlama run, a JSON proof would cost ~2.2e10 cycles and a `bincode` one ~2.8e9 (extrapolated, `docs/EVM-GAS-MODEL.md` §6.3). `Fixed` and `Claims` stay JSON. 23 `skip_serializing_if` fields in `compose.rs` stop them round-tripping through `bincode` or `postcard`, and they are ~1% of the synthetic corpus' bundle bytes (#45 measures TinyLlama's share).

**Rules.** `check_segment` in `sp1/sash-segment-check` implements them; the guest calls it. A `Fixed` that does not parse is rejected (`Malformed`). A `Plan::new` assertion on the parsed `params`, or any other panic in the verifier build (e.g. a decodable proof with fewer than three commitments indexes out of bounds before upstream's length check), is a rejection with no named variant: the guest exits 1 with the panic message, and the native check panics. Then the guest MUST check, in this order, and reject with the named variant:
1. **`TokenIdOutOfField`:** every id in `Fixed.tok_in` and `Fixed.tok_out` is `< P` (`P = 2^31 - 1`), before any Horner sum. SASH maps ids with `M31::from_u32_unchecked` (`m31_u32`, `prover/src/common.rs`), so `P + k` would alias `k`.
2. **`SecurityParams`:** `log_blowup == 1`, `n_queries == 100`, `pow_bits == 0`.
3. **`MissingLayer`:** `Fixed.layer` is `Some(l)` with `l < 2^16`.
4. **`UnboundEdge`:** `Plan::new(&fixed.params).binding(fixed.layer)` has no `EdgeState::Unbound` edge, and `params.prove_argmax_lax == false`. The `--gkr-*` flags are backend choices and stay allowed.
5. **`UnsoundDequant`:** every dequant `Spec` of the plan satisfies `Spec::sound()`, and `params.kmax <= KMAX_SOUND` (`21`, `prover/src/bf16.rs`).
6. **`NotAccepted`:** `Verdict::accepts()` holds on the verifier build's `Verdict`. This includes `root_ok` and `model_root_ok` (proof commitments 0 and 1 equal `Fixed.pre_root` and `Fixed.model_root`), `embed_tok_ok` and `head_tok_ok`. `shape_root_ok` is always `None` in the verifier build: the guest does not re-derive the shape root. `pre_root` is bound on-chain to the enrolled manifest (§1.3), and its correctness rests on enrollment: relying parties re-derive it offline with `zk-prover model-roots` or `zk-prover expect-root --shape-only --params <tag>.fixed.json --calibration <manifest>` ([SECURITY.md](SECURITY.md) §3). Undecodable claims or proof also reject here.
7. **`OpenSeam`:** `deq_out_open` and `rq_scale_open` are empty, and `ao_q_open` and `nn_open` are false. Upstream `accepts()` ignores them.

Relation draws, challenge counts and the GKR instance list come only from the `Plan` of the parsed `params`, never from the proof.

**`fixedHash`.** Decided: the guest commits the hash of the whole `Fixed` statement, and the manifest pins it. `fixedHash = sha256(serde_json::to_vec(&Fixed { tok_in: vec![], tok_out: vec![], ..parsed }))`, over the parsed struct, never the input bytes (serde drops unknown fields). It covers all of `Fixed`, not only `params`: `sizes_ok` skips tree 0, the verifier build never reads `layer` or `model`, and `block` is compared only with `rope.pos0` (`nn_pos_ok`, which passes when either is absent). The guest never reads a pin or a hash from its input, so one guest program serves every model.

**Segment public values.** After rule 7, the guest commits 226 bytes (`SegmentPublicValues` in `crates/sash-evm-primitives/src/segment.rs` encodes and decodes them) with `sp1_zkvm::io::commit_slice` (never `commit`, which is bincode, and never `sp1-zkvm`'s `blake3` feature: the aggregator verifies each segment proof with `verify_sp1_proof(vkey, sha256(publicValues))`, so the public-values digest must be sha256).

| Offset | Bytes | Field | Value |
|---:|---:|---|---|
| 0 | 2 | `segmentIndex` | `be16(Fixed.layer)`; `head` is `N - 1` (22 for TinyLlama) |
| 2 | 32 | `preRoot` | proof commitment 0 |
| 34 | 32 | `modelRoot` | proof commitment 1 |
| 66 | 32 | `residualInRoot` | the `(Axis::Layer, "in")` boundary-side root, commitment 4 |
| 98 | 32 | `residualOutRoot` | the `(Axis::Layer, "out")` boundary-side root, commitment 5; 32 zero bytes on `head`, which commits no out side |
| 130 | 32 | `fixedHash` | as above |
| 162 | 32 | `tokInHash` | `tokenHash(Fixed.tok_in)` (§1.3) on the segment with the embedding gadget (`B0L0`); else 32 zero bytes |
| 194 | 32 | `tokOutHash` | `tokenHash(Fixed.tok_out)` on the segment that composes the argmax (`head`); else 32 zero bytes |

- Roots are the 32 raw bytes of the `Blake2sHash`, not its hex string.
- Bytes `0..130` are the §1.3 segment claim. Once the planned `fixedHash` binding lands, bytes `0..162` are the claim.
- The K/V (`Axis::Token`) boundary-side roots (commitments 6 and up) are not output. That is sound only because `S = 32` is the single token block `B0`; a multi-block run needs them.

### 1.2 Aggregator guest (M3) and settlement

The aggregator verifies all `N` segment proofs and commits `publicDigest` (§1.3), taking each segment claim from its segment's public values (§1.1), `tokInHash` from segment `0` and `tokOutHash` from segment `N - 1`. The BN254 wrapper's public values are exactly the 32 bytes of `publicDigest`.

`VerifiableInferenceSettlement` recomputes `publicDigest` from calldata (`modelId`, `segments`, `tokIn`, `tokOut`) and verifies the segment proof over it before any grid check. Any mutation of a segment claim, `tokIn` or `tokOut` therefore fails the segment-proof check: the SP1 proof with `Sp1SashSegmentVerifier`, or the dev tag with `DevTranscriptSegmentVerifier` (`chainid == 31337` only, no cryptographic binding).

`publicDigest` does not fix where the prompt ends: `runId` (§1.3) binds the caller's `promptIds` / `answerIds` split, so one proof settles once per split that passes the grid checks (prompt lengths `19..32` for the TinyLlama fixture). Every split claims the same proved `tokIn` / `tokOut` forward pass, so no split claims more than the proof covers. Replay protection stays on `runId`: keying it on `publicDigest` would let anyone front-run a pending settlement under another split and block the intended one.

### 1.3 Encodings

All hashes are `sha256`. `be16` / `be32` are big-endian `uint16` / `uint32`. sha256 is used rather than keccak256: keccak256 would save only an estimated ~2k gas per settlement.

- **Token ids:** `tokenHash(ids) = sha256(word(ids[0]) || ... || word(ids[k-1]))`, where `word(id)` is the 32-byte big-endian encoding of the `uint32` id (28 zero bytes, then `be32(id)`). This is Solidity's `sha256(abi.encodePacked(uint32[]))`. The same encoding gives `tokInHash = tokenHash(tokIn)`, `tokOutHash = tokenHash(tokOut)`, `promptHash = tokenHash(promptIds)` and `answerHash = tokenHash(answerIds)`.
- **Segment claim** (`130` bytes): `claim(i) = be16(segmentIndex) || preRoot || modelRoot || residualInRoot || residualOutRoot`.
- **`manifestRootsHash`** (`64 * N` bytes hashed): `sha256(preRoot[0] || modelRoot[0] || ... || preRoot[N-1] || modelRoot[N-1])`.
- **`publicDigest`** (`100 + 130 * N` bytes hashed): `sha256(modelId || be32(N) || claim(0) || ... || claim(N-1) || tokInHash || tokOutHash)`.
- **`runId`:** `sha256(modelId || publicDigest || promptHash || answerHash)`.
- **`modelId`** (fixture convention): `sha256("<model>@<revision>:<profile>")`.

### 1.4 TinyLlama values

Taken from an unpublished SASH run that M2 checks against the proof bundles. The run is `AbdelStark/hallmark`'s TinyLlama case: its recipe (`make e2e-llm` at hallmark `675c99af66067d129dac30d6acc6612f4e392e68`) regenerates the 69 bundle files byte for byte, and `sp1/pins/tinyllama.sha256` pins them (`make bundles-check`). The case: `TinyLlama/TinyLlama-1.1B-Chat-v1.0 @ fe8a4ea1ffedaf415f4da2f062534de366a451e6`, profile `y-attnout-w12`, `N = 23`, `S = 32` (`25` prompt + `8` answer tokens ending at EOS `2`), prompt `"Name the capital of France in one short sentence."`, response `"The capital of France is Paris."`. `contracts/src/sash/TinyLlamaFixture.sol` is generated from `crates/sash-evm-primitives/src/tinyllama_fixture.rs` (`make vectors`).

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

### 2.1 Negative controls

Each control runs only in the suites it lists, and MUST fail there with the listed error: a revert in Foundry, a `GridError` in Rust, a `Reject` variant in the segment guest.

- **Grid-level controls** (1–5, 10–13, 16) run in both suites. Foundry submits each with a dev proof for its own `publicDigest`, so the revert comes from the grid check, not the proof check.
- **Settlement-level controls** (6, 7, 17–20, F) run in Foundry only. They exercise the segment verifier, verifier gating or replay protection, which have no Rust counterpart.
- **Guest-level controls** (G1–G8, the second table) run natively and in the SP1 executor (M2). Each makes the segment check return the listed `Reject` (§1.1) and the guest exit 1 with a panic message naming it. Native tests are in `sp1/sash-segment-check`: unit tests (`src/tests.rs`, on constructed statements, binding tables, dequant `Spec`s and `Verdict`s) and bundle tests (`tests/bundles.rs`, mutations of the synthetic corpus' positive bundles, run in the `sash` CI job).
- Numbers 8, 9, 14 and 15 are retired and not reused.

| # | Mutation of the valid TinyLlama run | Rust `GridError` | Solidity error | Suites | Tests (Rust · Foundry) |
|---|---|---|---|---|---|
| 1 | `segments[7].modelRoot` low bit flipped | `ManifestRootMismatch` | `ManifestRootMismatch(rootsHash, manifestRootsHash)` | Rust, Foundry | `control_01_forged_model_root` · `testNegative01_ForgedModelRootReverts` |
| 2 | `segments[0].preRoot` low bit flipped | `ManifestRootMismatch` | `ManifestRootMismatch(rootsHash, manifestRootsHash)` | Rust, Foundry | `control_02_forged_pre_root` · `testNegative02_ForgedPreRootReverts` |
| 3 | `segments[5].segmentIndex = 6` | `InvalidSegmentIndex { position: 5, got: 6 }` | `InvalidSegmentIndex(5, 6)` | Rust, Foundry | `control_03_out_of_order_segment_index` · `testNegative03_OutOfOrderSegmentIndexReverts` |
| 4 | `segments[10].residualOutRoot` low byte `^= 0xff` | `ResidualBoundaryMismatch { link: 10 }` | `ResidualBoundaryMismatch(10)` | Rust, Foundry | `control_04_broken_residual_boundary_link` · `testNegative04_BrokenResidualBoundaryLinkReverts` |
| 5 | `segments[22].residualOutRoot = 1` | `HeadMustNotCarryOutRoot` | `HeadMustNotCarryOutRoot()` | Rust, Foundry | `control_05_head_non_zero_residual_out_root` · `testNegative05_HeadNonZeroResidualOutRootReverts` |
| 6 | `tokIn[0] = promptIds[0] = 530`, original proof | — | `InvalidDevProofTag()` | Foundry | `testNegative06_MutatedTokInFailsSegmentProof` |
| 7 | `tokOut[0]` low bit flipped, original proof | — | `InvalidDevProofTag()` | Foundry | `testNegative07_MutatedTokOutFailsSegmentProof` |
| 10 | `promptIds[4] = 9999` | `PromptTokenMismatch { index: 4 }` | `PromptTokenMismatch(4)` | Rust, Foundry | `control_10_prompt_mismatch` · `testNegative10_PromptMismatchReverts` |
| 11 | `tokIn[27] = 1111` | `AutoregressiveLoopMismatch { step: 2 }` | `AutoregressiveLoopMismatch(2)` | Rust, Foundry | `control_11_broken_autoregressive_loop` · `testNegative11_BrokenAutoregressiveLoopReverts` |
| 12 | manifest `eosTokenId = 999` | `MissingEosTermination { got: 2, expected: 999 }` | `MissingEosTermination(2, 999)` | Rust, Foundry | `control_12_missing_eos` · `testNegative12_MissingEosReverts` |
| 13 | `answerIds[3] = 777` | `AnswerTokenMismatch { index: 3 }` | `AnswerTokenMismatch(3)` | Rust, Foundry | `control_13_answer_mismatch` · `testNegative13_AnswerMismatchReverts` |
| 16 | `segments` truncated to 22 | `InvalidSegmentCount { got: 22, expected: 23 }` | `InvalidSegmentCount(22, 23)` | Rust, Foundry | `control_16_truncated_segment_array` · `testNegative16_TruncatedSegmentArrayReverts` |
| 17 | proof bytes `0xdeadbeef` (dev verifier) | — | `InvalidDevProofTag()` | Foundry | `testNegative17_InvalidProofBytesReverts` |
| 18 | against `StubSp1Gateway` (checks the vkey and the proof bytes): a proof for a wrong `programVKey`; an adapter pinned to a wrong `programVKey`, with a proof valid for the gateway's | — | `StubSp1Gateway.BadSp1Proof()` | Foundry | `testNegative18_WrongSp1VKeyProofReverts` |
| 19 | dev verifier with `allowDevVerifier == false`; dev verifier on Sepolia (`11155111`) | — | `DevVerifierForbidden()`; `DevChainOnly(11155111)` | Foundry | `testNegative19_DevVerifierBlockedOnSepoliaAndWhenDisallowed` |
| 20 | the same run settled twice | — | `RunAlreadySettled(runId)` | Foundry | `testNegative20_DuplicateRunReplayReverts` |
| F | fuzz: one of `modelId`, any `SegmentClaim` field, any element of `tokIn` / `tokOut` / `promptIds` / `answerIds`, or one proof byte XORed with a non-zero mask; original dev proof, never re-derived | — | any revert | Foundry | `testFuzzSingleFieldMutationReverts` (seed fixed under `[fuzz]` in `contracts/foundry.toml`) |

| # | Mutation | `Reject` | Native tests (unit · bundle) | SP1 executor test |
|---|---|---|---|---|
| G1 | `fixed` not JSON; `fixed.json` truncated | `Malformed` | `malformed_fixed` · `mutations_malformed` | (#43) |
| G2 | an id `P`, `P + 3` or `u32::MAX` in `tok_in` or `tok_out`; TinyLlama's `tok_in` or `tok_out` with one id raised by `P` (on `sound_aligned`: `tok_in`) | `TokenIdOutOfField` | `token_id_out_of_field`, `tinyllama_token_raised_by_p` · `mutations_token_id_out_of_field` | (#43) |
| G3 | `log_blowup`, `n_queries` or `pow_bits` drifted | `SecurityParams` | `security_params_drifted` · `mutations_security_params` | (#43) |
| G4 | `layer` removed, or `65536` | `MissingLayer` | `missing_layer` · `mutations_missing_layer` | (#43) |
| G5 | an `Unbound` edge (`sound_gkrfive`, the four `skip_*` bundles, `head`, and `layer` 1 to 0 on `sound_aligned`); `params.prove_argmax_lax = true` | `UnboundEdge` | `unbound_edge`, `unbound_edge_argmax_lax` · `unbound_bundles_reject`, `mutations_unbound_edge` | (#43) |
| G6 | `params.kmax = 22`; an `int8` dequant `Spec` with `q_bits + rem_bits = 32` and `kmax <= 21` | `UnsoundDequant` | `unsound_dequant_kmax`, `unsound_dequant_spec` · `mutations_unsound_dequant` | (#43) |
| G7 | a Merkle decommitment hash byte flipped; a `claims` value changed; `pre_root` or `model_root` changed; `claims` or `proof` undecodable | `NotAccepted` | `not_accepted` · `mutations_not_accepted`, `undecodable_proof_is_not_accepted` | (#43) |
| G8 | `deq_out_open` or `rq_scale_open` non-empty, or `ao_q_open` or `nn_open` true, on an otherwise accepting `Verdict` (no corpus bundle has an open seam) | `OpenSeam` | `open_seam` · — | (#43) |
