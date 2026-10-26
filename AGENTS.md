# AGENTS.md: verifiable-inference-ethereum

Cross-harness operating rules for coding agents working in `verifiable-inference-ethereum`. `CLAUDE.md` is a symlink to this file: edit **AGENTS.md**.

> Read this file and [SPEC.md](SPEC.md) before modifying code, contracts, or documentation.
>
> **Sources of truth.** The code is the strongest source of truth for how the repository works: where this file or `docs/` disagrees with the code, fix the file to say how the code works now and name the edit in your reply. `SPEC.md` is the protocol contract, derived from the pinned upstream SASH revision: where the code and `SPEC.md` disagree, neither silently wins; record the disagreement in §9 and ask.
>
> This file records rules: the rule, its exact constants and where they live, and one line of why when the why is what stops an agent undoing it. Protocol detail lives in `SPEC.md`, the threat model in `SECURITY.md`, design narrative in `docs/`.

## 1. Mission and technical scope

`verifiable-inference-ethereum` implements an Ethereum EVM and Solidity verification stack for **SASH verifiable LLM inference** (`sg-ai-safety-hub/zk-inference-prover` pinned at `eb95bfa887299d1637550228d42086c27cbb2b3b`, built on `NitrooZK-stwo` / Stwo Circle STARKs), progressing from local Foundry unit and Anvil dev-mode verification to the public **Hoodi** testnet (`560048`, the exit-gate chain), with **Sepolia** (`11155111`) optional.

Reference LLM target: `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`), SASH profile `y-attnout-w12`, `S = 32` tokens (`25` prompt tokens + `8` generated answer tokens ending in EOS `2`, split across `23` segments: `B0L0..B0L21` + `head`).

## 2. Non-negotiable engineering invariants

1. **No overclaiming.** An accepted verification proves **quantized execution integrity** for one declared run of an enrolled `(pre_root, model_root)` manifest under SASH's `y-attnout-w12` relation. Never claim output safety, alignment, floating-point equivalence, or zero-knowledge hiding (SASH inner proofs use `LOG_BLOWUP = 1` without trace blinding, and public settlement opens prompt/response tokens).
2. **Explicit cryptographic assumption tracking.** Raw SASH segment proofs rely on `BLAKE2s` Circle STARK + GKR (~100 conjectured security bits, post-quantum candidate). Compressing 23 wide segments through an SP1 or RISC Zero BN254 Groth16 wrapper shifts the outer assumption to classical BN254 pairing soundness, Groth16 SRS ceremony integrity, and zkVM circuit soundness. Keep `VerifierKind` explicit in every contract, event, and receipt.
3. **Hard separation of dev-mode mocks.** `DevTranscriptSegmentVerifier` is strictly restricted to `block.chainid == 31337` (Foundry / Anvil) AND blocked whenever `VerifiableInferenceSettlement.allowDevVerifier == false`. Never weaken either guard.
4. **Bit-exact cross-language parity.** Rust (`crates/sash-evm-primitives`) and Solidity (`contracts/src/`) must agree bit-for-bit on the grid checks and on the sha256 `manifestRootsHash`, token hashes and `publicDigest` byte layouts (`SPEC.md` §1.3). Any change to Rust primitives must regenerate `contracts/src/sash/TinyLlamaFixture.sol` (`make vectors`, generated from `tinyllama_fixture.rs`; never hand-edit it) and verify it (`make check`).
5. **EVM consensus discipline.** Osaka (mainnet, Hoodi) binds:
   - every settlement transaction stays safely below the `EIP-7825` cap of `16,777,216` gas;
   - it accounts for the `EIP-7623` calldata floor, where gas is `max(standard + execution, floor)` and the floor is `10` / `40` gas per zero / non-zero byte;
   - every contract fits `EIP-170`'s `24,576` bytes.

   Amsterdam (live on Sepolia, provisional until mainnet activation) is measured alongside:
   - the cap counts execution gas only (`EIP-8037`);
   - the floor is `64` gas per byte (`EIP-7976`);
   - `24,576` bytes stays the project's code size limit, although Amsterdam allows `65,536` (`EIP-7954`).
6. **Outward actions require explicit human approval.** Never run `git push`, open PRs/issues, broadcast transactions to public RPCs/testnets, invoke hosted prover networks, download multi-GB weights, or spend paid compute without explicit approval in the active session. A request to work issue #N is that approval for pushing its `issue-N-*` branch and opening its PR (§6), and for nothing else.
7. **Zero secrets or personal data.** Never commit private keys, RPC URLs with API keys, `.env` files, or large binary proof archives (`runs/`, `*.tar.gz`, `*.safetensors`).

## 3. Engineering principles

User-decided rules with the same bar as the invariants.

- **Do not preserve backward compatibility.** Remove obsolete paths instead of adding compatibility layers, fallbacks or migrations.
- **Choose the simplest implementation that fully meets the current requirements.** No speculative abstractions, configuration or indirection.
- **Grow the system in layers**: smallest end-to-end version first, each capability on top of a stack that already works (the `docs/ROADMAP.md` milestones are those layers). Never trade a working stack for unfinished complexity.
- **Keep components modular and concerns clearly separated.**
- **Prefer established, well-maintained libraries**; lean on the dependencies already in the workspace before writing your own or adding crates. Check a library's docs/types before assuming it lacks a capability.
- **Make architectural decisions for the long term.** No stopgaps meant to be replaced later.

## 4. Repository layout

- `SPEC.md`: Canonical protocol, wire format, contract ABI, gas budget, and 20-case negative-control specification.
- `SECURITY.md`: Threat model, cryptographic assumption boundary, and audit checklist.
- `docs/ARCHITECTURE.md`: Two-layer hybrid design (zkVM segment compression + native Solidity SASH grid verifier).
- `docs/EVM-GAS-MODEL.md`: Empirical TinyLlama-1.1B SASH sizing and `EIP-7825` / `EIP-7623` arithmetic.
- `docs/ROADMAP.md`: Milestones `M0` through `M4` (Foundry -> Anvil -> Sepolia/Hoodi).
- `crates/sash-evm-primitives`: Pure Rust reference SASH 23-segment grid checks and sha256 settlement digests, and the TinyLlama-1.1B fixture and `TinyLlamaFixture.sol` generator.
- `contracts/`: Foundry Solidity verifier suite (`SashModelRegistry`, `SashGridVerifier`, `Sp1SashSegmentVerifier`, `DevTranscriptSegmentVerifier`, `VerifiableInferenceSettlement`), deployment scripts, and negative-control test suite.

## 5. Testing and verification

Before declaring any change complete, run:

```bash
make check
```

`make check` enforces `cargo fmt`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` (including the real TinyLlama-1.1B 23-segment grid, its digests pinned to `SPEC.md` §1.4, and the grid-level negative controls 1 to 5, 10 to 13 and 16; controls 6, 7 and 17 to 20 are settlement-level and exist only in Foundry), byte-exact sync of the generated `contracts/src/sash/TinyLlamaFixture.sol` with the Rust export (`export_vectors` run `--locked`), and, whenever Foundry is installed in `PATH`, `forge fmt --check`, `forge build --locked --sizes` (fails if a submodule differs from `contracts/foundry.lock`), `forge lint --deny warnings` and `forge test`.

- **Write tests when a change touches the protocol**: grid checks and settlement guards. Assert against `SPEC.md` and the primary-source vectors, not against the implementation.
- **Don't add tests for docs, cosmetic changes, trivial wiring or config.**
- **A failing invariant test is a real regression: fix the code, never weaken the test.** Never regenerate the fixture (`make vectors`) or edit a primary-source vector to make a failing test pass; the fixture changes only with an intended Rust primitive change (§2.4).
- **A grid-level negative control lands in both suites**, Rust and Foundry, never on one side only. Settlement-level controls (token binding, proof bytes, verifier gating, replay) run in Foundry only (`SPEC.md` negative-control section).

Other targets (`make help`): `make rust-check` (Rust gates only), `make vectors` (regenerate the fixture), `make forge-test` (`forge fmt --check && forge build --locked --sizes && forge lint --deny warnings && forge test -vvv --gas-report`), `make anvil-demo` (`DeployDevAnvil.s.sol` against chain `31337`), `make clean`.

## 6. Working an issue

When asked to work/implement/do/resolve issue #N:

- **Read it first** (`gh issue view N`), then **implement the actual code**; never just change its GitHub status.
- **Respect every invariant in this file**; write tests per §5 and run `make check`.
- **Branch + PR**: branch `issue-N-short-slug`, commit, push, `gh pr create` referencing the issue with `Refs #N` (not `Closes`, unless asked). Do **NOT** merge, close or replace the PR, and do **NOT** close the issue: the human decides.
- **No agent/tool branding** in branch names or PR titles. **Keep the PR description short**: what changed, how to verify, any AGENTS.md edits.

## 7. Maintaining this file

- **You are a scribe of the user's decisions, not an author of them.** Update this file when the user has **explicitly** decided something that changes an invariant, command, wire format or architecture rule, or when it disagrees with the code as it stands (the header's rule). Never record a rule you inferred or think is a good idea; never document a transient state as permanent.
- **Two zones, two bars.** *Stable rules* (§2 to §7): edit only on an explicit, confirmed user decision or to match the code as it stands, never to match a change you are making, and call the edit out prominently in your reply. *Current state* (§8): may be updated to reflect what now exists.
- **Put a rule in the right home.** Agent rules live here; protocol and wire format in `SPEC.md`; threat model and assumption boundary in `SECURITY.md`; design, gas and milestones in `docs/`. State a rule once and reference it from elsewhere.
- **Record how it works now, not its history.** A rule, its constants, its location, and one line of why when the why matters. Never what was decided before, reverted, or when; no measurements, review chronology or rejected alternatives, except a rejected alternative an agent would plausibly re-propose, in one line.
- **Surface every edit** in your reply. **When in doubt, do not edit**: ask or mention it. Keep edits minimal and consistent with the existing structure.

## 8. Current state (mutable)

- **CI** (`.github/workflows/ci.yml`) runs on pushes to `main` and on every PR, in two jobs:
  - `rust`: `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, and the fixture byte-sync check (`--locked`).
  - `solidity` (Foundry `v1.8.6`): `forge fmt --check`, `forge build --locked --sizes` (`foundry.lock`, `EIP-170`), `forge lint --deny warnings`, `forge test -vvv --gas-report`, and a dry run of `DeployDevAnvil.s.sol`.
  - **`ci.yml` spells out its steps; it does not call `make`.** A gate added to `make check` must be added to `ci.yml` too, and the other way round. `make check` skips the Anvil dry run (`make anvil-demo` runs it locally).

## 9. Discrepancies to confirm

Rust, Solidity and the fixture agree with each other on every item below; `SPEC.md` says otherwise. Each item is decided, and `SPEC.md` is wrong until the named issue lands.

- **Negative controls**:
  - `SPEC.md` §2 requires every control to revert in both the Rust and Foundry suites, but Rust has the grid-level controls only (1 to 5, 10 to 13 and 16).
  - Decided: settlement-level controls run in Foundry only (§5), and SPEC is amended (#9).
