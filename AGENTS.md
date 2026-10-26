# AGENTS.md: verifiable-inference-ethereum

Cross-harness operating rules for coding agents working in `verifiable-inference-ethereum` (`CLAUDE.md` mirrors this contract).

## 1. Mission and technical scope

`verifiable-inference-ethereum` implements an Ethereum EVM and Solidity verification stack for **SASH verifiable LLM inference** (`sg-ai-safety-hub/zk-inference-prover` pinned at `eb95bfa887299d1637550228d42086c27cbb2b3b`, built on `NitrooZK-stwo` / Stwo Circle STARKs), progressing from local Foundry unit and Anvil dev-mode verification to public Ethereum testnets (**Sepolia** `11155111` and **Hoodi** `560048`).

Reference LLM target: `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`), SASH profile `y-attnout-w12`, `S = 32` tokens (`25` prompt tokens + `8` generated answer tokens ending in EOS `2`, split across `23` segments: `B0L0..B0L21` + `head`).

## 2. Non-negotiable engineering invariants

1. **No overclaiming.** An accepted verification proves **quantized execution integrity** for one declared run of an enrolled `(pre_root, model_root)` manifest under SASH's `y-attnout-w12` relation. Never claim output safety, alignment, floating-point equivalence, or zero-knowledge hiding (SASH inner proofs use `LOG_BLOWUP = 1` without trace blinding, and public settlement opens prompt/response tokens).
2. **Explicit cryptographic assumption tracking.** Raw SASH segment proofs rely on `BLAKE2s` Circle STARK + GKR (~100 conjectured security bits, post-quantum candidate). Compressing 23 wide segments through an SP1 or RISC Zero BN254 Groth16 wrapper shifts the outer assumption to classical BN254 pairing soundness, Groth16 SRS ceremony integrity, and zkVM circuit soundness. Keep `VerifierKind` explicit in every contract, event, and receipt.
3. **Hard separation of dev-mode mocks.** `DevTranscriptSegmentVerifier` is strictly restricted to `block.chainid == 31337` (Foundry / Anvil) AND blocked whenever `VerifiableInferenceSettlement.allowDevVerifier == false`. Never weaken either guard.
4. **Bit-exact cross-language parity.** Rust (`crates/sash-evm-primitives`) and Solidity (`contracts/src/`) must agree bit-for-bit on `M31`, `CM31`, `QM31`, `Blake2s`, `reduce_to_m31`, `Blake2sM31Channel` (`0x00` domain byte, `< 2P` rejection sampling, `draw_secure_felts(2)` single-batch semantics), `CirclePoint`, and `QM31` Horner evaluation (`asc` on `B0L0`, `desc` on `head`). Any change to Rust primitives must regenerate and verify `contracts/test/vectors/tinyllama_23seg_fixture.json` (`make check`).
5. **EVM consensus discipline.** Every settlement transaction must stay safely below `EIP-7825` (`16,777,216` gas per transaction cap) and account for `EIP-7623` (`40` gas per non-zero calldata byte floor) and `EIP-170` (`24,576` bytes per contract).
6. **Outward actions require explicit human approval.** Never run `git push`, open PRs/issues, broadcast transactions to public RPCs/testnets, invoke hosted prover networks, download multi-GB weights, or spend paid compute without explicit approval in the active session.
7. **Zero secrets or personal data.** Never commit private keys, RPC URLs with API keys, `.env` files, or large binary proof archives (`runs/`, `*.tar.gz`, `*.safetensors`).

## 3. Repository layout

- `SPEC.md`: Canonical protocol, wire format, contract ABI, gas budget, and 20-case negative-control specification.
- `SECURITY.md`: Threat model, cryptographic assumption boundary, and audit checklist.
- `docs/ARCHITECTURE.md`: Two-layer hybrid design (zkVM segment compression + native Solidity SASH grid & Fiat-Shamir verifier) and native Circle STARK path.
- `docs/EVM-GAS-MODEL.md`: Empirical TinyLlama-1.1B SASH sizing and `EIP-7825` / `EIP-7623` arithmetic.
- `docs/ROADMAP.md`: Milestones `M0` through `M4` (Foundry -> Anvil -> Sepolia/Hoodi) and `M5` (Stwo-in-Stwo recursion).
- `crates/sash-evm-primitives`: Pure Rust reference implementation of `M31`/`CM31`/`QM31`, RFC 7693 `BLAKE2s-256`, `Blake2sM31Channel`, `CirclePoint`, `Blake2sM31MerkleVerifier`, SASH 23-segment grid verifier, and TinyLlama-1.1B vector exporter.
- `crates/sash-repack`: Deterministic validator and `SASHBIN1` binary repacker for SASH `(fixed, claims, proof)` segment bundles.
- `contracts/`: Foundry Solidity verifier suite (`M31`, `CM31`, `QM31`, `Blake2s`, `Blake2sM31Channel`, `Blake2sM31MerkleVerifier`, `CirclePoint`, `SashModelRegistry`, `SashGridVerifier`, `Sp1SashSegmentVerifier`, `DevTranscriptSegmentVerifier`, `VerifiableInferenceSettlement`), deployment scripts, and 20-mutation test suite.

## 4. Verification workflow

Before declaring any change complete, run:

```bash
make check
```

`make check` enforces `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test --workspace` (including Stwo reference vectors, real TinyLlama-1.1B 23-segment Fiat-Shamir Horner vectors, and all 20 negative-control mutations), byte-exact vector sync with `contracts/test/vectors/tinyllama_23seg_fixture.json`, and `forge build && forge test` whenever Foundry is installed in `PATH`.
