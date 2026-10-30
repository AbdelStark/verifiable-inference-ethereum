# Protocol and Contract Specification (`SPEC.md`)

Version: `0.6.0` (2026-10-30)
Pinned upstream SASH revision: `sg-ai-safety-hub/zk-inference-prover @ eb95bfa887299d1637550228d42086c27cbb2b3b`
SP1: sp1-sdk `6.10.0`, circuit `v6.1.0`; on-chain verifier: sp1-contracts `v6.1.1` `contracts/src/v6.1.0/SP1VerifierPlonk.sol`.
Stwo as run by SASH: the vendored `nitroo/` copy in `zk-inference-prover @ eb95bfa`. It equals `AntChainOpenLabs/NitrooZK-stwo @ 41c2a05bdb1fc3e3762bc7804108e8bc269bb6fc` for the field, channel, Merkle and FRI primitives, and differs in `nitroo/crates/stwo/src/core/pcs/verifier.rs`, `nitroo/crates/stwo/src/core/verifier.rs`, `nitroo/crates/stwo/src/core/air/components.rs` and `nitroo/crates/constraint-framework`.
Files patched against `eb95bfa` for the segment guests' verifier build (§1.1), by `sp1/patches/`: `nitroo/crates/stwo/build.rs`, `nitroo/crates/constraint-framework/src/lib.rs`, `nitroo/crates/stwo/src/core/mod.rs`, `nitroo/crates/stwo/src/core/lookups/{mod,gkr_verifier,sumcheck,utils}.rs` (new), `nitroo/crates/stwo/src/prover/lookups/{gkr_verifier,sumcheck,utils}.rs`, `prover/src/gkr_key_tile.rs`, `prover/src/gkrlink.rs`, and for the `zk-prover` cfg-split `prover/Cargo.toml`, `prover/build.rs`, `prover/src/bf16.rs`, `prover/src/boundary.rs`, `prover/src/common.rs`, `prover/src/compose.rs`, `prover/src/encoding.rs`, `prover/src/gadgets/argmax/mod.rs`, `prover/src/gadgets/attention/air.rs`, `prover/src/gadgets/attention/mod.rs`, `prover/src/gadgets/attention/trace.rs`, `prover/src/gadgets/attention/witness.rs`, `prover/src/gadgets/attn_out/air.rs`, `prover/src/gadgets/attn_out/claims.rs`, `prover/src/gadgets/attn_out/mod.rs`, `prover/src/gadgets/attn_out/trace.rs`, `prover/src/gadgets/dequant/claims.rs`, `prover/src/gadgets/dequant/mod.rs`, `prover/src/gadgets/dequant/trace.rs`, `prover/src/gadgets/embed/mod.rs`, `prover/src/gadgets/freivalds/claims.rs`, `prover/src/gadgets/freivalds/mod.rs`, `prover/src/gadgets/freivalds/trace.rs`, `prover/src/gadgets/requant/mod.rs`, `prover/src/gadgets/requant/tables.rs`, `prover/src/gadgets/residual_stream/mod.rs`, `prover/src/gadgets/residual_stream/witness.rs`, `prover/src/gadgets/rope/mod.rs`, `prover/src/gadgets/rope/witness.rs`, `prover/src/layer.rs`, `prover/src/lib.rs`, `prover/src/relations.rs`, `prover/src/util.rs`, for the `verify_composed` split `prover/src/compose.rs`, for the `count-ops` counter `nitroo/crates/stwo/Cargo.toml`, `nitroo/crates/stwo/src/core/vcs/{blake2_hash,blake2_merkle,mod}.rs` and `nitroo/crates/stwo/src/core/vcs/count_ops.rs` (new), and for the token-vector transcript binding (patch 0006, prover and verifier) `prover/src/common.rs` and `prover/src/compose.rs`. Patched for the synthetic corpus' prover only, outside the verifier build (patch 0007): `prover/src/cli.rs` and `prover/src/layer.rs`.

## 1. Proved statement and settlement digests

This section is the requirement the M2 segment guests and the M3 aggregator guest must meet. The segment guest (`sp1/segment-guest`) implements §1.1; the aggregator guest does not exist yet. The contracts implement §1.3 and §2 as written.

### 1.1 Segment guests (M2)

Each segment guest verifies one SASH segment (`<tag>.fixed.json`, `<tag>.claims.json`, `<tag>.proof.json`) with a verifier-only build of `verify_composed` (`prover/src/compose.rs`) at `eb95bfa` plus the token binding below, and commits the segment's public values.

**Verifier build.** Upstream `verify_composed` cannot run in an SP1 guest: it calls `std::time::Instant::now`, its crate enables stwo's `prover` feature (which needs `array_chunks`, removed from Rust in 1.90), and nitroo's stwo `build.rs` runs CUDA cmake on every non-macOS host. The verifier build MAY differ from upstream only by:
- stwo's `build.rs` gated on the `prover` feature;
- constraint-framework's `mod cuda_domain` and its re-export gated on `prover`;
- the GKR verifier (`gkr_verifier.rs`, the verifier half of `sumcheck.rs`, `utils.rs`) moved from `stwo::prover::lookups` to `stwo::core::lookups`, with re-exports and import fixes;
- `zk-prover` cfg-split under a default-on `prover` feature;
- `Instant::now` and `Verdict.verify_secs` cfg-gated out of the verifier build;
- `shape_root_check`, `shape_root_from_params` and their `catch_unwind` cfg-gated out of the verifier build, with `verify_composed` binding upstream's no-calibration value `(None, None, 0.0, None)` in their place;
- `verify_composed` split into byte decoding and `verify_composed_parsed`, a verify over the parsed `(Fixed, Claims, Proof)` and the `Plan` of `Fixed.params`, the byte entry building that `Plan` and delegating to it unchanged. The segment check builds the `Plan` once, for rules 4 and 5, and passes it;
- a `count-ops` feature on stwo, off by default and enabled only by a native host measurement build, never a guest: `core/vcs/count_ops.rs` counts BLAKE2s compressions per hash at finalize, and `blake2_merkle.rs` and `blake2_hash.rs` take their `Blake2s256` from it under the feature, on the same line as their `blake2` import, so that no other line moves (their panic sites embed their line in the guest ELF);
- the token binding (patch 0006), the only change that changes behaviour: the published token vectors mixed into the Fiat-Shamir channel, identically by the prover and by `verify_composed_parsed` (below).

The difference MUST be reproducible as a reviewed patch series against `eb95bfa`, and the header lists every patched file. Any other change needs an edit to this list in the change that makes it.

**Synthetic token vectors (patch 0007, prover only).** No upstream synthetic layer has an embedding table or an argmax key column, so no synthetic bundle would publish a token vector and G9 could not run on the corpus. Patch 0007 adds two witness-construction switches to the prover's CLI, both inside the `prover`-gated `impl Layer` in `prover/src/layer.rs`, so the verifier build does not change: `--synth-embed` (a layer-0 table whose row `(3 + 7t) mod 64` is the wiring family's input row `t`, every other row zero) and `--synth-argmax` (the head's key column as the order key of the `lm_head` dequant's own output, each row emitting its first maximum, as a dump's `key_head.u16` and `tok_head.i32` hold). The corpus bundles `embed` and `head_argmax` use them (`sp1/scripts/sash-synthetic.sh`).

**Token binding (patch 0006).** Upstream `eb95bfa` never mixes `Fixed.tok_in` or `Fixed.tok_out` into the channel: `embed_tok_ok` and `head_tok_ok` compare their Horner sums under `w_residual_stream` with `claims.embed_tok` and `claims.head_tok`, but `w` is drawn from a transcript the published vectors never entered. Holding a valid bundle, anyone can replay the transcript to `w`, change any positions of a published vector and solve one QM31 equation (four linear equations over M31) for four slack positions; every id stays `< P`, the claims, the roots and `fixedHash` stay as they were, and upstream accepts. G9 (§2.1) runs this forgery against the patched verifier. Decided: patch it here, on both sides, and follow upstream's fix when it ships.
- `token_transcript_words(tok_in, tok_out)` (`prover/src/compose.rs`) is `len(tok_in) || tok_in || len(tok_out) || tok_out` as `u32` words, the ids as the exact `u32`s the statement publishes (so `P + k` and `k` differ) and each length as a `u32`, or nothing when both vectors are empty.
- `verify_composed_parsed` mixes those words of `fixed.tok_in` and `fixed.tok_out` with `mix_u32s` right after committing tree 2 and before `Rels::draw`, so every relation element and `w_residual_stream` depend on them. The prover computes the two vectors it publishes before its first relation draw, mixes the same words at the same position (`Session::mix_u32s`, recorded as `Op::MixU32s` for its own replay) and publishes the same two vectors in `Fixed`.
- A segment that publishes neither vector (every TinyLlama segment but `B0L0` and `head`, and every synthetic corpus bundle but `embed` and `head_argmax`) mixes nothing: its transcript, its proof and every digest upstream pins for it are unchanged. A bundle with a token vector proved without the patch rejects at rule 6, and one proved with it rejects under upstream.
- Not a digest of the whole `Fixed`: its roots are tree 0's and tree 1's, already in the transcript, `fixedHash` pins the rest on-chain (§1.3), and mixing it would need a canonical encoding of `Params`.

The differential tests' oracle (`sp1/sash-oracle`) runs upstream's `verify_composed` with patch 0006 alone (`sp1/vendor/sash-oracle`, default features), so they check that the extraction (patches 0001 to 0005) preserves the verdict.

**Guest input.** The guest reads three buffers with `sp1_zkvm::io::read_vec`, in this order:
- `fixed`: the bytes of `<tag>.fixed.json`, decoded as `serde_json`;
- `claims`: the bytes of `<tag>.claims.json`, decoded as `serde_json`;
- `proof`: the `StarkProof` the host parses from `<tag>.proof.json` and re-encodes as `bincode` 1.3.3 (`bincode::serialize`: fixed-width little-endian integers, `u64` lengths).

The guest decodes the buffers itself and calls the parsed entry of `verify_composed`. A `fixed` that does not decode is `Malformed`, and a `claims` or `proof` that does not decode rejects at rule 6. Whatever the wire format, `fixedHash` hashes the re-serialized parsed `Fixed`. The re-encoding is outside the trusted base: the guest verifies whatever proof it decodes.

These formats come from measurements (`docs/EVM-GAS-MODEL.md` §6.2). Decoding `sound_aligned`'s `StarkProof` costs 31.0 cycles per JSON byte, 9.6 per `bincode` byte and 11.8 per `postcard` byte. Re-encoded, the proof is 0.386 of the JSON size in `bincode` and 0.445 in `postcard`. That is 732M cycles as JSON, 87M as `bincode` and 124M as `postcard`. Over the TinyLlama run, decoding the 23 `bincode` proofs measures 2.5e9 cycles, 5.3% of the guest's 4.8e10 (`docs/EVM-GAS-MODEL.md` §7); as JSON they would cost ~2.3e10 at 31.0 cycles per byte. `Fixed` and `Claims` stay JSON. 23 `skip_serializing_if` fields in `compose.rs` stop them round-tripping through `bincode` or `postcard`. They are ~1% of the synthetic corpus' bundle bytes and 3.0% of TinyLlama's, whose `Fixed` and `Claims` decode in 1.4e9 cycles, 3.0% of the guest's.

**Rules.** `check_segment` in `sp1/sash-segment-check` implements them; the segment guest calls it, and on a rejection panics with `segment rejected: <variant>`. A `Fixed` that does not parse is rejected (`Malformed`). A `Plan::new` assertion on the parsed `params`, or any other panic in the verifier build (e.g. a decodable proof with fewer than three commitments indexes out of bounds before upstream's length check), is a rejection with no named variant: the guest exits 1 with the panic message, and the native check panics. Then the guest MUST check, in this order, and reject with the named variant:
1. **`TokenIdOutOfField`:** every id in `Fixed.tok_in` and `Fixed.tok_out` is `< P` (`P = 2^31 - 1`), before any Horner sum. SASH maps ids with `M31::from_u32_unchecked` (`m31_u32`, `prover/src/common.rs`), so `P + k` would alias `k`.
2. **`SecurityParams`:** `log_blowup == 1`, `n_queries == 100`, `pow_bits == 0`.
3. **`MissingLayer`:** `Fixed.layer` is `Some(l)` with `l < 2^16`.
4. **`UnboundEdge`:** `Plan::new(&fixed.params).binding(fixed.layer)` has no `EdgeState::Unbound` edge, and `params.prove_argmax_lax == false`. The `--gkr-*` flags are backend choices and stay allowed.
5. **`UnsoundDequant`:** every dequant `Spec` of the plan satisfies `Spec::sound()`, and `params.kmax <= KMAX_SOUND` (`21`, `prover/src/bf16.rs`).
6. **`NotAccepted`:** `Verdict::accepts()` holds on the verifier build's `Verdict`. This includes `root_ok` and `model_root_ok` (proof commitments 0 and 1 equal `Fixed.pre_root` and `Fixed.model_root`), `embed_tok_ok` and `head_tok_ok`, which bind `Fixed.tok_in` and `Fixed.tok_out` to the ids the proof committed only because the token binding mixes them before `w_residual_stream` is drawn. `shape_root_ok` is always `None` in the verifier build: the guest does not re-derive the shape root. `pre_root` is bound on-chain to the enrolled manifest (§1.3), and its correctness rests on enrollment: relying parties re-derive it offline with `zk-prover model-roots` or `zk-prover expect-root --shape-only --params <tag>.fixed.json --calibration <manifest>` ([SECURITY.md](SECURITY.md) §3). Undecodable claims or proof also reject here.
7. **`OpenSeam`:** `deq_out_open` and `rq_scale_open` are empty, and `ao_q_open` and `nn_open` are false. Upstream `accepts()` ignores them.
8. **`NotSingleBlock`:** `Fixed.block == Some(0)` and `params.c == 0`: the statement is the single token block `B0`, with no prefix keys. The public values omit the K/V (`Axis::Token`) boundary roots, which bind a prefix's keys and values to the block that wrote them, so a prefix would read keys and values bound to nothing. With `block == Some(0)`, `nn_pos_ok` (rule 6) also fixes `rope.pos0` at 0. The rule reads only the statement but runs last because it guards the public values: `check_verdict` derives them after rule 7, and `check_segment` returns them only once rule 8 holds.

Relation draws, challenge counts and the GKR instance list come only from the `Plan` of the parsed `params`, never from the proof.

**`fixedHash`.** Decided: the guest commits the hash of the whole `Fixed` statement, and the manifest pins it. `fixedHash = sha256(serde_json::to_vec(&Fixed { tok_in: vec![], tok_out: vec![], ..parsed }))`, over the parsed struct, never the input bytes (serde drops unknown fields). It covers all of `Fixed`, not only `params`: `sizes_ok` skips tree 0, the verifier build never reads `layer` or `model`, and `block` is compared only with `rope.pos0` (`nn_pos_ok`, which passes when either is absent). The guest never reads a pin or a hash from its input, so one guest program serves every model: `fixedHash` is the last field of the segment claim, and `manifestRootsHash` (§1.3) compares it with the enrolled pin on-chain.

**Segment public values.** After rule 8, the guest commits 226 bytes (`SegmentPublicValues` in `crates/sash-evm-primitives/src/segment.rs` encodes and decodes them) with `sp1_zkvm::io::commit_slice` (never `commit`, which is bincode, and never `sp1-zkvm`'s `blake3` feature: the aggregator verifies each segment proof with `verify_sp1_proof(vkey, sha256(publicValues))`, so the public-values digest must be sha256).

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
- Bytes `0..162` are the §1.3 segment claim.
- The K/V (`Axis::Token`) boundary-side roots (commitments 6 and up) are not output. That is sound only for the single token block `B0` (`S = 32` for TinyLlama), which rule 8 enforces; a multi-block run needs them.

### 1.2 Aggregator guest (M3)

The aggregator guest verifies the `N` segment proofs inside SP1 and commits `publicDigest` (§1.3). Its PLONK wrap is the proof `Sp1SashSegmentVerifier` checks on-chain (§2).

**Guest input.** The guest reads two buffers with raw `sp1_zkvm::io::read_vec`, in this order, never with `io::read`:
- `modelId`: exactly 32 bytes;
- `segmentValues`: `226 * N` bytes, `N >= 1`: the segment public values (§1.1) of segments `0` to `N - 1`, concatenated in segment order. `values_i = segmentValues[226i..226(i+1)]`.

After them the host passes the `N` compressed segment proofs with `SP1Stdin::write_proof(proof, vk)`, in the same segment order. The guest never reads them: the recursion consumes them (binding, below).

**Segment vkey.** `SEGMENT_VKEY: [u32; 8]` is a compile-time constant of the guest, equal to `hash_u32` in `sp1/segment-guest.vkey`. It is never read from the input: a vkey from stdin would let the prover substitute any program for the segment guest. A segment guest change that moves its vkey must update `SEGMENT_VKEY`, which moves the aggregator's own vkey.

**Checks and commitment.** The guest MUST:
1. Reject, in this order, with the named variant, by panicking with `aggregate rejected: <name>` (the guest exits non-zero; SP1 can still prove that execution on request (`expected_exit_code`), but the proof then attests a non-zero `exitCode`, which `SP1VerifierPlonk` rejects with `InvalidExitCode`, below):
   - **`ModelIdLength`:** `modelId` is not 32 bytes;
   - **`SegmentValuesLength`:** `segmentValues` is empty, or its length is not a multiple of 226.

   These are controls A1 and A2 (§2.1).
2. For each `i` in `0..N`, in order, call `verify_sp1_proof(&SEGMENT_VKEY, &sha256(values_i))` (sp1-lib and sp1-zkvm feature `verify`).
3. Compute `publicDigest` (§1.3, byte for byte) with `claim(i) = values_i[0..162]`, `tokInHash = values_0[162..194]` and `tokOutHash = values_{N-1}[194..226]`.
4. Commit exactly those 32 bytes with `sp1_zkvm::io::commit_slice`, and nothing else: never `commit` (bincode adds a length prefix), and never `sp1-zkvm`'s `blake3` feature (the public-values digest must be the sha256 the on-chain verifier hashes, below).

**Binding rests on SP1's recursion.** `verify_sp1_proof` checks nothing when the guest runs: it folds `(SEGMENT_VKEY, sha256(values_i))` into the guest's deferred-proofs digest, which the guest commits when it halts (`sp1-zkvm-6.10.0` `src/syscalls/verify.rs`). Only proving enforces the binding to real proofs: SP1 v6's deferred-proof recursion checks the stdin proofs, in stdin order, against that chain of `(vk, sha256(public values))` pairs, and requires each to be a complete compressed proof (`is_complete == 1`) that exited `0` (`sp1-recursion-circuit-6.10.0` `src/machine/deferred.rs`). A verifying aggregator proof therefore attests that `N` complete segment-guest proofs under `SEGMENT_VKEY` committed exactly `segmentValues`, through that recursion alone. The SP1 executor does not check the binding: it treats the `VERIFY_SP1_PROOF` syscall as a no-op (`sp1-core-executor-6.10.0` `src/minimal/ecall.rs`), and with deferred verification on, it verifies each stdin proof against its own vk but never matches the guest's calls against them (`sp1-prover-6.10.0` `src/worker/prover/execute.rs`). A wrong vkey, a changed public-value byte, a reordered or a missing proof all execute; only binding controls B1 and B2 (§2.1), which prove, can reject them ([SECURITY.md](SECURITY.md) §1).

**Not checked in the guest, and why that is sound.**
- **`modelId`** is an unchecked input: the guest only hashes it into `publicDigest`. Settlement binds it: it recomputes `publicDigest` from calldata, `modelId` included, and checks the claims against the manifest enrolled for that `modelId` (§2).
- **Segment indices, residual-stream links and manifest roots** (`preRoot`, `modelRoot`, `fixedHash`) are checked on-chain by `SashGridVerifier` over the claims `publicDigest` commits (§2), not in the guest.
- **The token-hash fields of segments `1` to `N - 2`, `tokOutHash` of segment `0` and `tokInHash` of segment `N - 1`** are ignored. Which segment embeds the tokens or takes the argmax is fixed by its `Fixed`, hence by its `fixedHash` and the manifest: `B0L0` and `head` for TinyLlama (§1.1).
- **A zero `tokInHash` or `tokOutHash`** (segment `0` or `N - 1` without the gadget) cannot settle: settlement recomputes `publicDigest` with `tokenHash` of the calldata vectors, and no vector's `tokenHash` is 32 zero bytes without a sha256 preimage of zero.

**The wrap.** The aggregator's compressed proof is wrapped as SP1's BN254 PLONK proof by sp1-sdk `6.10.0` (circuit `v6.1.0`), with proof nonce `0` (the default; never `with_proof_nonce`) and no TEE proof. `Sp1SashSegmentVerifier` passes the pinned `SP1VerifierPlonk` (header):
- `programVKey`: its immutable `SASH_AGGREGATOR_VKEY`, the aggregator's `vk.bytes32()`, pinned in `sp1/aggregator-guest.vkey`;
- `publicValues`: exactly the 32 bytes of `publicDigest` (`abi.encodePacked(publicDigest)`);
- `proofBytes`: `SP1ProofWithPublicValues::bytes()` (`sp1-sdk-6.10.0` `src/proof.rs`), the verifier selector followed by the encoded proof (`sp1-recursion-gnark-ffi-6.10.0` `go/sp1/utils.go`), 964 bytes. A TEE proof, when set, is prepended to them and would move the selector, so none is set.

| Offset | Bytes | Field | Value |
|---:|---:|---|---|
| 0 | 4 | `selector` | `VERIFIER_HASH()[0..4]` = `0x5a093a2f` |
| 4 | 32 | `exitCode` | `0` |
| 36 | 32 | `vkRoot` | `VK_ROOT()` = `0x002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352` |
| 68 | 32 | `nonce` | `0` |
| 100 | 864 | PLONK proof | gnark's `MarshalSolidity` encoding |

`SP1VerifierPlonk.verifyProof` reverts with `WrongVerifierSelector` on another selector, `InvalidExitCode` on a non-zero `exitCode` and `InvalidVkRoot` on another `vkRoot`, then checks the PLONK proof against five public inputs, in this order: `programVKey`, `sha256(publicValues) & (2^253 - 1)`, `exitCode`, `vkRoot`, `nonce`.

### 1.3 Encodings

All hashes are `sha256`. `be16` / `be32` are big-endian `uint16` / `uint32`. sha256 is used rather than keccak256: keccak256 would save only an estimated ~2k gas per settlement.

- **Token ids:** `tokenHash(ids) = sha256(word(ids[0]) || ... || word(ids[k-1]))`, where `word(id)` is the 32-byte big-endian encoding of the `uint32` id (28 zero bytes, then `be32(id)`). This is Solidity's `sha256(abi.encodePacked(uint32[]))`. The same encoding gives `tokInHash = tokenHash(tokIn)`, `tokOutHash = tokenHash(tokOut)`, `promptHash = tokenHash(promptIds)` and `answerHash = tokenHash(answerIds)`.
- **Segment claim** (`162` bytes): `claim(i) = be16(segmentIndex) || preRoot || modelRoot || residualInRoot || residualOutRoot || fixedHash`, the first 162 bytes of segment `i`'s public values (§1.1).
- **`manifestRootsHash`** (`96 * N` bytes hashed): `sha256(preRoot[0] || modelRoot[0] || fixedHash[0] || ... || preRoot[N-1] || modelRoot[N-1] || fixedHash[N-1])`.
- **`publicDigest`** (`100 + 162 * N` bytes hashed): `sha256(modelId || be32(N) || claim(0) || ... || claim(N-1) || tokInHash || tokOutHash)`.
- **`runId`:** `sha256(modelId || publicDigest || promptHash || answerHash)`.
- **`modelId`** (fixture convention): `sha256("<model>@<revision>:<profile>")`.

### 1.4 TinyLlama values

Taken from a SASH run whose 23 proof bundles the segment guest verifies in the SP1 executor (`make sp1-execute`, `docs/EVM-GAS-MODEL.md` §7); `sp1/pins/tinyllama.fixed.sha256` pins each segment's `fixedHash`. The run is `AbdelStark/hallmark`'s TinyLlama case, proved with the token binding (§1.1): its recipe (`make e2e-llm` at hallmark `675c99af66067d129dac30d6acc6612f4e392e68`), with `sp1/patches/0006-bind-token-vectors-into-transcript.patch` applied to the SASH source (`patch -p1 -F0`) before the prover build, produced the 69 bundle files, and `sp1/pins/tinyllama.sha256` pins them (`make bundles-check`). They are published as the public Hugging Face dataset `0xchqrles/sash-tinyllama-bundles` at commit `7284cdba1353dfd441fb4a244aabd0c9bfb9dcd5` (`make bundles-download`, `sp1/scripts/tinyllama-bundles.sh`). The recipe without the patch regenerates the earlier, unbound bundles byte for byte; with it, the 21 segments that publish no token vector come out byte-identical to those, and only `B0L0`'s and `head`'s `claims.json` and `proof.json` differ. Their `fixed.json`, trace roots and tokens are unchanged, so every `fixedHash` pin and the fixture stay the same. The case: `TinyLlama/TinyLlama-1.1B-Chat-v1.0 @ fe8a4ea1ffedaf415f4da2f062534de366a451e6`, profile `y-attnout-w12`, `N = 23`, `S = 32` (`25` prompt + `8` answer tokens ending at EOS `2`), prompt `"Name the capital of France in one short sentence."`, response `"The capital of France is Paris."`. `contracts/src/sash/TinyLlamaFixture.sol` is generated from `crates/sash-evm-primitives/src/tinyllama_fixture.rs` (`make vectors`), and regenerated byte for byte from the 23 bundles: `make sp1-fixture-check` renders it from the public values the segment guest commits on them (`sp1-host fixture-from-bundles`). That comparison does not cover `modelId`, `EOS_TOKEN_ID` or the prompt and response text, which come from enrollment, not from the proofs, and which `fixture-from-bundles` takes from `tinyllama_fixture.rs`.

- `modelId` = `sha256("TinyLlama/TinyLlama-1.1B-Chat-v1.0@fe8a4ea1ffedaf415f4da2f062534de366a451e6:y-attnout-w12")` = `0x3def3a0832d3d82a82283200010c182132ac7d9eec681b772a95834f29e43c2a`
- `manifestRootsHash` = `0x5537541600ba4323d8f27606150c5583e689ca527590d33cd3d3b20bb04de646`
- `tokInHash` = `0x3e683ee8603b85a4f5407e1af5f20a437ddadc9c098acc6aac357d533d3f32cb`
- `tokOutHash` = `0x44c6f339d896d66ea25d0ca6e15214f596ef5c701db79be94232be51d29b6b1e`
- `publicDigest` = `0xcf52e19e82f82680f0098ab435b52e22512f3d634bd7a947d32abc76d067b3ef`
- `promptHash` = `0xb4d5730f21772191057fb4600dc73dfdf0350fab6996c7b12d1d1ca7b83b9ec1`
- `answerHash` = `0xaed36ba9347820959ae88d81e8c45a733e404b199fb3c12a463e2301253934c8`
- `runId` = `0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28`

## 2. Solidity contract interfaces and negative controls

1. **`SashModelRegistry.sol`:** Enrolls `ModelManifest { uint16 numSegments; uint16 seqLen; uint32 eosTokenId; bytes32 manifestRootsHash; bool active; }`, with `manifestRootsHash` as in §1.3.
2. **`SashGridVerifier.sol`:** Verifies the segment count, indices `0..N-1`, `manifestRootsHash`, the 22 residual-stream boundary root equalities (`segments[i].residualOutRoot == segments[i+1].residualInRoot`), `segments[N-1].residualOutRoot == bytes32(0)`, `tokIn.length == tokOut.length == seqLen`, the token window (`P + A - 1 == seqLen`), the `promptIds` prefix match, autoregressive loop closure (`tokIn[P + j] == tokOut[P - 1 + j]`), the `answerIds` match, and `tokOut[P + A - 2] == eosTokenId`. It does not range-check token ids: the guest statement covers them (§1.1).
3. **`VerifiableInferenceSettlement.sol`:** Checks `allowDevVerifier` against `segmentVerifier.verifierKind()`, checks replay protection (`settledRuns[runId]`), verifies `segmentVerifier.verifyAggregatedSegments(publicDigest, proofBytes)` with `publicDigest` as in §1.3, invokes `SashGridVerifier.verifyGrid`, records settlement, and emits `InferenceVerified`.
4. **`Sp1SashSegmentVerifier.sol`:** Binds the SP1 verifier and `SASH_AGGREGATOR_VKEY` immutably, and calls the verifier as in §1.2. Its constructor reverts with `ZeroVerifierAddress()` on a zero verifier address and `ZeroProgramVKey()` on a zero vkey, then reads the verifier's `VERIFIER_HASH()` and `VK_ROOT()` and reverts with `WrongSp1Verifier(verifierHash, vkRoot)` unless they are `SP1_PLONK_VERIFIER_HASH = 0x5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac` and `SP1_VK_ROOT = 0x002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352`, the v6.1.0 `SP1VerifierPlonk`'s ([SECURITY.md](SECURITY.md) §6). `VK_ROOT` is checked as well because Sepolia's broken v6.1.0 PLONK verifier returns the pinned `VERIFIER_HASH` with another `VK_ROOT`. A contract without these getters, such as the `SP1VerifierGateway`, or an address without code makes the constructor revert with empty returndata, which the high-level call bubbles up.

`VerifiableInferenceSettlement` recomputes `publicDigest` from calldata (`modelId`, `segments`, `tokIn`, `tokOut`) and verifies the segment proof over it before any grid check. Any mutation of a segment claim, `tokIn` or `tokOut` therefore fails the segment-proof check: the SP1 proof with `Sp1SashSegmentVerifier`, or the dev tag with `DevTranscriptSegmentVerifier` (`chainid == 31337` only, no cryptographic binding).

The segment guest accepts only the token vectors its segment proof committed (§1.1, token binding), so the segment proofs fix `tokIn`, `tokOut` and therefore `publicDigest`. `publicDigest` does not fix where the prompt ends: `runId` (§1.3) binds the caller's `promptIds` / `answerIds` split, so one proof settles once per split that passes the grid checks (prompt lengths `19..32` for the TinyLlama fixture). Every split claims the same proved `tokIn` / `tokOut` forward pass, so no split claims more than the proof covers. Replay protection stays on `runId`: keying it on `publicDigest` would let anyone front-run a pending settlement under another split and block the intended one.

### 2.1 Negative controls

Each control runs only in the suites it lists, and MUST fail there with the listed error: a revert in Foundry, a `GridError` in Rust, a `Reject` variant in the segment guest, a named rejection in the aggregator guest, or, for a binding control, no verifying proof.

- **Grid-level controls** (1–5, 10–13, 16, 21) run in both suites. Foundry submits each with a dev proof for its own `publicDigest`, so the revert comes from the grid check, not the proof check.
- **Settlement-level controls** (6, 7, 17–20, 23–25, 28–30, F) run in Foundry only. They exercise the segment verifier, verifier gating or replay protection, which have no Rust counterpart. Controls 23–25 and 28–30 run against the real v6.1.0 `SP1VerifierPlonk`, compiled from the `sp1-contracts` submodule under the repository's compiler settings: 28–30 deploy `Sp1SashSegmentVerifier` against another verifier, and 23–25 settle through the adapter on `SP1VerifierPlonk`, with `allowDevVerifier == false` and a non-zero `programVKey`, a 964-byte `proofBytes` whose header is the pinned one (§1.2: selector `0x5a093a2f`, `exitCode` 0, `vkRoot` `SP1_VK_ROOT`, `nonce` 0), followed by 864 zero bytes, with one header field mutated, so each reverts before the pairing check.
- **Guest-level controls** (G1–G10, the second table) run natively and in the SP1 executor (M2). Each makes the segment check return the listed `Reject` (§1.1) and the guest exit 1 with a panic message naming it. Native tests are in `sp1/sash-segment-check`: unit tests (`src/tests.rs`, on constructed statements, binding tables, dequant `Spec`s and `Verdict`s) and bundle tests (`tests/bundles.rs`, mutations of the corpus bundles that pass rules 1 to 7 (`sound_aligned`, `aligned_gkr5`, `embed`, `head_argmax`), run in the `sash` CI job). The executor tests (`sp1/sp1-host/tests/segment_guest.rs`, run in the `sp1` or `sp1-cached` CI job) run the segment guest on every corpus bundle and every bundle mutation of `tests/bundles.rs`; the unit tests' constructed controls and G8, which no bundle reaches, run natively only. On the real TinyLlama bundles, the `sp1-bundles` CI job (`sp1/scripts/sp1-execute.sh` on the published set, with the Docker-built ELF from the same run) checks the accepting path: the guest exits 0 and commits public values equal to native `check_segment`'s, with `fixedHash` equal to its pin, on `B0L0`, `B0L10` and `head` on a PR, and on all 23 segments on pushes to `main`, the weekly schedule and a manual run, which also run `sp1/scripts/sp1-fixture-check.sh` (the script behind `make sp1-fixture-check`, §1.4). On every run it reaches, `sp1-execute.sh` (`sp1-host controls`) also runs G2, G7, G9 and G10 on modified copies of `B0L0`, `B0L10` and `head`, natively and in the executor: `tok_in[0] + P` on `B0L0` and `tok_out[0] + P` on `head` (G2), a queried value of `B0L10`'s proof and an `ao_out` value of its claims changed (G7), the Horner-preserving token forgeries (G9), and `block` removed on `B0L10` and `block` 1 on `head` (G10). G9's forgeries (`sash-segment-check`'s `forge` feature, never in the guest) are solved under the `w` the bundle was proved with: the one, of upstream's transcript and patch 0006's, under which the honest vector's Horner sum is the claimed one, and a bundle where neither gives it fails the control instead of rejecting. On the corpus they must find patch 0006's.
- **Aggregator-level controls** (A1, A2, the third table) run natively, against the aggregator's pure check, and in the SP1 executor. Each makes the check return the listed rejection (§1.2) and the guest exit non-zero with the panic message `aggregate rejected: <name>`.
- **Binding controls** (B1, B2, the fourth table) run only when proving, on a GPU, because no executor can reach them: the SP1 executor treats `verify_sp1_proof` as a no-op and never matches the guest's calls against the stdin proofs (§1.2). Each mutates one half of the `(vk, sha256(public values))` pair that the deferred-proof recursion checks, at `N = 1` on `B0L10`, with the host's pre-GPU checks skipped (`sp1-host aggregate-control`). Both run once, in compressed mode, next to the honest `N = 1` compressed aggregation of `B0L10`, which must verify. A control passes only on no verifying proof: proving fails, or the proof fails verification. A timeout is inconclusive, not a pass. The host's pre-check that the segment vk's `hash_u32` equals `SEGMENT_VKEY` is a host guard, not a control: it compares two pins and never reaches the recursion.
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
| 21 | `segments[22].fixedHash` low bit flipped | `ManifestRootMismatch` | `ManifestRootMismatch(rootsHash, manifestRootsHash)` | Rust, Foundry | `control_21_forged_fixed_hash` · `testNegative21_ForgedFixedHashReverts` |
| 23 | `proofBytes[0]` set to `0x00` | — | `SP1Verifier.WrongVerifierSelector(0x00093a2f, 0x5a093a2f)` | Foundry | `testNegative23_WrongVerifierSelectorReverts` |
| 24 | `exitCode` (`proofBytes[4..36]`) set to `1` | — | `SP1Verifier.InvalidExitCode()` | Foundry | `testNegative24_NonZeroExitCodeReverts` |
| 25 | `vkRoot` (`proofBytes[36..68]`) set to the broken Sepolia PLONK verifier's `0x00995aa7…2251` | — | `SP1Verifier.InvalidVkRoot()` | Foundry | `testNegative25_WrongVkRootReverts` |
| 28 | `Sp1SashSegmentVerifier` deployed against an `SP1VerifierGateway` (the submodule's, owned by the test); against an address without code | — | a revert with empty returndata | Foundry | `testNegative28_GatewayRejectedAtDeploy` |
| 29 | `Sp1SashSegmentVerifier` deployed against the v6.1.0 `SP1VerifierGroth16` | — | `WrongSp1Verifier(0x4388a21c…e696, 0x002f850e…5352)` | Foundry | `testNegative29_Groth16VerifierRejectedAtDeploy` |
| 30 | `Sp1SashSegmentVerifier` deployed against the test-only `GetterStubPlonkVerifier`, its getters set to Sepolia's `V6_1_0_BROKEN_SP1_VERIFIER_PLONK`'s: `VERIFIER_HASH` `0x5a093a2f…7fac`, `VK_ROOT` `0x00995aa7…2251` | — | `WrongSp1Verifier(0x5a093a2f…7fac, 0x00995aa7…2251)` | Foundry | `testNegative30_BrokenVkRootPlonkVerifierRejectedAtDeploy` |
| F | fuzz: one of `modelId`, any `SegmentClaim` field, any element of `tokIn` / `tokOut` / `promptIds` / `answerIds`, or one proof byte XORed with a non-zero mask; original dev proof, never re-derived | — | any revert | Foundry | `testFuzzSingleFieldMutationReverts` (seed fixed under `[fuzz]` in `contracts/foundry.toml`) |

| # | Mutation | `Reject` | Native tests (unit · bundle) | SP1 executor test |
|---|---|---|---|---|
| G1 | `fixed` not JSON; `fixed.json` truncated | `Malformed` | `malformed_fixed` · `mutations_malformed` | `mutations_malformed` |
| G2 | an id `P`, `P + 3` or `u32::MAX` in `tok_in` or `tok_out`; TinyLlama's `tok_in` or `tok_out` with one id raised by `P` (on `sound_aligned`: `tok_in`) | `TokenIdOutOfField` | `token_id_out_of_field`, `tinyllama_token_raised_by_p` · `mutations_token_id_out_of_field` | `mutations_token_id_out_of_field` |
| G3 | `log_blowup`, `n_queries` or `pow_bits` drifted | `SecurityParams` | `security_params_drifted` · `mutations_security_params` | `mutations_security_params` |
| G4 | `layer` removed, or `65536` | `MissingLayer` | `missing_layer` · `mutations_missing_layer` | `mutations_missing_layer` |
| G5 | an `Unbound` edge (`sound_gkrfive`, the four `skip_*` bundles, `head`, and `layer` 1 to 0 on `sound_aligned`); `params.prove_argmax_lax = true` | `UnboundEdge` | `unbound_edge`, `unbound_edge_argmax_lax` · `unbound_bundles_reject`, `mutations_unbound_edge` | `unbound_bundles_reject`, `mutations_unbound_edge` |
| G6 | `params.kmax = 22` (on `sound_aligned`: both halves of rule 5, and with every matmul `limbs`, sound at any `kmax`, the `kmax` half only); an `int8` dequant `Spec` with `q_bits + rem_bits = 32` and `kmax <= 21` (on `sound_aligned`: `down` as `fold` with `fold_rem_bits = 24`, the `Spec` half only) | `UnsoundDequant` | `unsound_dequant_kmax`, `unsound_dequant_spec` · `mutations_unsound_dequant` | `mutations_unsound_dequant` |
| G7 | a Merkle decommitment hash byte flipped; a `claims` value changed; `pre_root` or `model_root` changed; `claims` or `proof` undecodable | `NotAccepted` | `not_accepted` · `mutations_not_accepted`, `undecodable_proof_is_not_accepted` | `mutations_not_accepted`, `undecodable_proof_is_not_accepted` |
| G8 | `deq_out_open` or `rq_scale_open` non-empty, or `ao_q_open` or `nn_open` true, on an otherwise accepting `Verdict` (no corpus bundle has an open seam: the bundle test opens each on `sound_aligned`'s own `Verdict` and runs `check_verdict`, the step `check_segment` returns its public values from) | `OpenSeam` | `open_seam` · `open_seam_after_the_verifier` | — |
| G9 | `tok_in[25]` set to 450 with `tok_in[1..=4]` solved (on `embed`; on the real `B0L0`), or `tok_out[24]` set to 450 with `tok_out[0..=3]` solved (on `head_argmax`; on the real `head`), so that the Horner sum under the bundle's own `w_residual_stream` stays the claimed one (§1.1, token binding). These bases are proved with patch 0006, so a verifier without it rejects them forged or honest; on a bundle proved without the patch, the same forgery solved under upstream's `w` is accepted by such a verifier. G9 fails on the corpus if the bundle's `w` is not the one drawn after the vectors are mixed (the prover dropped the mix), and `token_bundles_verify` fails if the verifier drops it (both bases then reject at rule 6 instead of at rule 8, G10) | `NotAccepted` | — · `mutations_not_accepted` (`embed_tok_in_horner_forged`, `head_argmax_tok_out_horner_forged`; `token_bundles_verify` checks that `check_verdict` gives both bases' public values and that rule 8 is the first rule to reject them) | `mutations_not_accepted`, `token_bundles_verify`; `make sp1-execute`'s controls `tok_in_horner_forged`, `tok_out_horner_forged` (natively and in the executor, in the `sp1-bundles` CI job) |
| G10 | `block` absent or not `0`, or `params.c > 0`: `sound_aligned`, `aligned_gkr5`, `embed` and `head_argmax` (block 0 with a 128-key prefix, `--ctx 128`; all four pass rules 1 to 7) and `layer` 1 to 2 on `sound_aligned` | `NotSingleBlock` | `not_single_block` · `sound_aligned_is_not_single_block`, `aligned_gkr5_is_not_single_block`, `token_bundles_verify`, `mutations_not_single_block` | `prefix_bundles_reject`, `token_bundles_verify`, `mutations_not_single_block`; `make sp1-execute`'s controls `block_removed`, `head_block_1` (natively and in the executor, in the `sp1-bundles` CI job) |

| # | Mutation | Rejection | Native test | SP1 executor test |
|---|---|---|---|---|
| A1 | `modelId` of 31 or 33 bytes | `ModelIdLength` | — (with the aggregator guest) | — (with the aggregator guest) |
| A2 | `segmentValues` empty; `226 * 23 - 1` bytes | `SegmentValuesLength` | — (with the aggregator guest) | — (with the aggregator guest) |

| # | Mutation | Expected | Tests (GPU) | Run |
|---|---|---|---|---|
| B1 | `B0L10`'s real compressed proof in stdin; the aggregator's `segmentValues` are `B0L10`'s public values with byte 10 flipped (inside `preRoot`, bytes `2..34`, §1.1) | no verifying proof: proving fails, or the proof fails verification | `sp1-host aggregate-control --kind pv-byte` | not yet run |
| B2 | in stdin, in place of `B0L10`'s proof, a compressed proof of the control-only echo guest that committed `B0L10`'s exact 226 public-value bytes, so its public values match and only its vk differs from `SEGMENT_VKEY`; `segmentValues` unchanged | no verifying proof, as B1 | `sp1-host aggregate-control --kind vk` | not yet run |
