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
2. **Explicit cryptographic assumption tracking.** Raw SASH segment proofs rely on `BLAKE2s` Circle STARK + GKR (~100 conjectured security bits, post-quantum candidate). Compressing 23 wide segments through an SP1 v6 BN254 wrapper (Groth16 or PLONK, chosen at M3 start) shifts the outer assumption to classical BN254 pairing soundness, the integrity of that wrap's setup ceremony, and zkVM circuit soundness. Keep `VerifierKind` explicit in every contract, event, and receipt.
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

- `SPEC.md`: Canonical protocol: the proved statement, sha256 digest layouts, contract ABI, and the negative-control table (§2.1).
- `SECURITY.md`: Threat model, cryptographic assumption boundary, and audit checklist.
- `docs/ARCHITECTURE.md`: Two-layer hybrid design (zkVM segment compression + native Solidity SASH grid verifier).
- `docs/EVM-GAS-MODEL.md`: Sizes from the unpublished TinyLlama-1.1B SASH run, the Osaka / Amsterdam network rules, measured gas and estimates.
- `docs/ROADMAP.md`: Milestones `M0` through `M4` (Foundry -> Anvil -> Sepolia/Hoodi).
- `docs/STARK-NATIVE-ROUTE.md`: Research note comparing STARK-native routes to Ethereum (Cairo / S-two / SHARP) with the SP1 BN254 wrap.
- `crates/sash-evm-primitives`: Pure Rust reference SASH 23-segment grid checks and sha256 settlement digests, and the TinyLlama-1.1B fixture and `TinyLlamaFixture.sol` generator.
- `sp1/`: SP1 v6.10.0 workspace, separate from the root one, with its own `Cargo.lock`: the segment guest (`segment-guest`: reads the three `SPEC.md` §1.1 buffers, calls `check_segment` and commits the 226-byte public values, or panics naming the `Reject` variant), `sash-segment-check` (`check_segment`: the `SPEC.md` §1.1 rules around the no-prover `verify_composed`, as a native library the guest calls; its unit tests run everywhere, its corpus bundle and differential tests are `#[ignore]`d and run by `make sash-synthetic`), `stwo-bench` (nitroo's Stwo verifier primitives in cycle-tracker regions) and their host crate (`sp1-host`, which builds the guests; its binary runs the segment guest on one bundle in the SP1 executor (`execute`) and prints a guest's vkey (`vkey`), with every run bounded by its `CYCLE_LIMIT`; its `segment_guest` executor suite is `#[ignore]`d and runs in the `sp1` CI job on the corpus); `segment-guest.elf.sha256` and `segment-guest.vkey`, the segment guest's ELF sha256 and vkey from the x86_64 Docker build (`scripts/docker-guests.sh`), re-committed whenever a guest-linked file changes (`sash-evm-primitives`, the vendored tree and its patches, `Cargo.lock`); `patches/` (the reviewed patch series against SASH `eb95bfa`, `SPEC.md` §1.1) and `scripts/vendor-sash.sh`, which builds the gitignored `vendor/sash` from SASH's sha256-pinned tarball plus those patches (the root `Cargo.toml` excludes it); `sash-oracle` (its own workspace on SASH's nightly toolchain: upstream `verify_composed`, default features, printing the `Verdict`, `accepts()` and the binding table as JSON, the reference for differential tests); `scripts/sash-synthetic.sh`, which builds upstream SASH's prover and generates the synthetic bundle corpus into the gitignored `testdata/synthetic/`, checked against the committed `testdata/synthetic.sha256` and `testdata/synthetic.oracle.json` (each run also writes the fresh ones beside them as gitignored `*.fresh` files, which CI uploads); plus `pins/tinyllama.sha256`, the sha256 pins of the 69 TinyLlama proof bundle files (`make bundles-check`).
- `bench/stwo-cairo/`: S-two unit-cost microbenchmarks (BLAKE2s, QM31 mul-add, M31 ingestion) as a Scarb executable, and `run.py`, which clones `stwo-cairo` at its pin, proves them and reproduces the tables in `docs/STARK-NATIVE-ROUTE.md`. Neither `make check` nor CI runs it.
- `contracts/`: Foundry Solidity verifier suite (`SashModelRegistry`, `SashGridVerifier`, `Sp1SashSegmentVerifier`, `DevTranscriptSegmentVerifier`, `VerifiableInferenceSettlement`), deployment scripts, and the Foundry rows of the `SPEC.md` §2.1 negative-control table.

## 5. Testing and verification

Before declaring any change complete, run:

```bash
make check
```

`make check` enforces `cargo fmt`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` (including the real TinyLlama-1.1B 23-segment grid, its digests pinned to `SPEC.md` §1.4, and the Rust rows of the `SPEC.md` §2.1 negative-control table; settlement-level controls run in Foundry only), byte-exact sync of the generated `contracts/src/sash/TinyLlamaFixture.sol` with the Rust export (`export_vectors` run `--locked`), and, whenever Foundry is installed in `PATH`, `forge fmt --check`, `forge build --locked --sizes` (fails if a submodule differs from `contracts/foundry.lock`), `forge lint --deny warnings`, `forge test` (with `contracts/snapshots/` deleted first) and a check that the regenerated gas snapshots match the index, then `make anvil-demo`; whenever `cargo-prove` is installed, `make sp1-check`; and, on an x86_64 host where the SP1 `v6.10.0` image pinned by digest in `sp1/scripts/docker-guests.sh` is also present locally (it is never pulled), `make sp1-docker`, which fails unless the Docker-built segment guest matches the committed `sp1/segment-guest.elf.sha256` and `sp1/segment-guest.vkey`. sp1-build runs the image as `linux/amd64`, and its emulated build hangs on arm64 hosts.

- **Gas snapshots are committed** in `contracts/snapshots/`. An intended gas change fails `make check` until the regenerated snapshots are staged; give the cause in the PR. A Foundry or solc bump also changes them; name the bump as the cause. Keep the measured figures in `docs/EVM-GAS-MODEL.md` §4 and §5 equal to the snapshots and to the table `make anvil-demo` prints; its deployment and `enrollModel` rows aren't gated, so check them whenever a contract changes. A test's `assertLe` budget is raised only for an intended change, with the reason in the PR.
- **Write tests when a change touches the protocol**: grid checks and settlement guards. Assert against `SPEC.md` and the primary-source vectors, not against the implementation.
- **Don't add tests for docs, cosmetic changes, trivial wiring or config.**
- **A failing invariant test is a real regression: fix the code, never weaken the test.** Never regenerate the fixture (`make vectors`) or edit a primary-source vector to make a failing test pass; the fixture changes only with an intended Rust primitive change (§2.4).
- **A grid-level negative control lands in both suites**, Rust and Foundry, never on one side only. Settlement-level controls (token binding, proof bytes, verifier gating, replay) run in Foundry only. A guest-level control runs natively and in the SP1 executor, never on one side only (`SPEC.md` §2.1 table).

Other targets (`make help`): `make rust-check` (Rust gates only), `make vectors` (regenerate the fixture), `make forge-test` (`forge fmt --check && forge build --locked --sizes && forge lint --deny warnings`, then `forge test -vvv --gas-report` on a fresh `contracts/snapshots/` and the snapshot check), `make anvil-demo` (`contracts/script/anvil-settle.sh`: mines `DeployDevAnvil.s.sol` on a local `anvil --hardfork osaka --enable-tx-gas-limit` node and checks the receipts, the `InferenceVerified` event and `verifyAndSettle`'s gas against its snapshot), `make sp1-vendor` (`sp1/scripts/vendor-sash.sh`: SASH `eb95bfa` from its sha256-pinned tarball plus `sp1/patches/*.patch`, applied with `patch -F0`, into `sp1/vendor/sash`), `make sp1-check` (runs `sp1-vendor`; checks nitroo's `stwo` and `stwo-constraint-framework` (in `sp1/vendor/sash/nitroo`) and SASH's `zk-prover` library (in `sp1/vendor/sash`) without the `prover` feature on stable `1.99.1`; then in `sp1/`: `cargo fmt -- --check`, `cargo test --locked --workspace`, which builds the guests and runs `sash-segment-check`'s unit tests (the segment guest's executor suite is `#[ignore]`d: it needs the corpus and runs in the `sp1` CI job), `cargo test --locked -p sp1-host --features profiling` for `stwo-bench`'s cycle-tracker regions, and `cargo clippy --locked --workspace --all-targets -- -D warnings`, plus the same for `sp1-host` with `--features profiling`, which need the built ELFs; needs SP1 v6.10.0 and `protoc`), `make sp1-docker` (`sp1/scripts/docker-guests.sh`: `cargo prove build --docker --tag v6.10.0 --locked --workspace-directory ../..` for `segment-guest` and `stwo-bench` in the image pinned by digest (`SP1_DOCKER_IMAGE`), printing their ELF sha256 and the segment guest's vkey (`sp1-host vkey`), and failing unless both equal `sp1/segment-guest.elf.sha256` and `sp1/segment-guest.vkey`), `make sash-synthetic` (`sp1/scripts/sash-synthetic.sh`, Linux with the CUDA 12.9 toolkit only, no GPU: builds upstream SASH's prover from `sp1/vendor/sash`, runs upstream's whole test suite (`cargo test --release --locked`, as upstream's CI does; none of it needs model data, and its pinned `synthetic_digest` is included), regenerates the eight-bundle synthetic corpus with the oracle's verdict per bundle, and fails unless they match the committed records and the expected unbound edges; then runs `sash-segment-check`'s bundle and differential tests on that corpus against the oracle (`cargo test --locked -p sash-segment-check -- --ignored` in `sp1/`, with `SASH_SYNTHETIC_CORPUS` and `SASH_ORACLE`; they fail, never skip, without them); `make check` runs it only on such hosts, which adds an uncached release build of about 30 minutes, and prints a NOTE elsewhere), `make bundles-check` (checks the 69 TinyLlama proof bundle files in `BUNDLES`, default the gitignored `runs/tinyllama`, against `sp1/pins/tinyllama.sha256`; not run by `make check` or CI, which have no bundles), `make clean`.

## 6. Working an issue

When asked to work/implement/do/resolve issue #N:

- **Read it first** (`gh issue view N`), then **implement the actual code**; never just change its GitHub status.
- **Respect every invariant in this file**; write tests per §5 and run `make check`.
- **Branch + PR**: branch `issue-N-short-slug`, commit, push, `gh pr create` referencing the issue with `Refs #N` (not `Closes`, unless asked). Do **NOT** merge, close or replace the PR, and do **NOT** close the issue: the human decides.
- **One worktree per issue; parallel where they don't conflict.** Work each issue in its own git worktree. Run issues that touch disjoint files concurrently; stack a dependent issue's branch on the branch it depends on. Use subagents for the roles an issue needs (implementing, research, independent review, critique), and have the coordinating session brief them, resolve conflicts in shared files (`Makefile`, `ci.yml`, `AGENTS.md`, `SPEC.md`) and report to the user.
- **No agent/tool branding** in branch names or PR titles. **Keep the PR description short**: what changed, how to verify, any AGENTS.md edits.

## 7. Maintaining this file

- **You are a scribe of the user's decisions, not an author of them.** Update this file when the user has **explicitly** decided something that changes an invariant, command, wire format or architecture rule, or when it disagrees with the code as it stands (the header's rule). Never record a rule you inferred or think is a good idea; never document a transient state as permanent.
- **Two zones, two bars.** *Stable rules* (§2 to §7): edit only on an explicit, confirmed user decision or to match the code as it stands, never to match a change you are making, and call the edit out prominently in your reply. *Current state* (§8): may be updated to reflect what now exists.
- **Put a rule in the right home.** Agent rules live here; protocol and wire format in `SPEC.md`; threat model and assumption boundary in `SECURITY.md`; design, gas and milestones in `docs/`. State a rule once and reference it from elsewhere.
- **Record how it works now, not its history.** A rule, its constants, its location, and one line of why when the why matters. Never what was decided before, reverted, or when; no measurements, review chronology or rejected alternatives, except a rejected alternative an agent would plausibly re-propose, in one line.
- **Surface every edit** in your reply. **When in doubt, do not edit**: ask or mention it. Keep edits minimal and consistent with the existing structure.

## 8. Current state (mutable)

- **CI** (`.github/workflows/ci.yml`) runs on pushes to `main` and on every PR, in four jobs:
  - `rust`: `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, and the fixture byte-sync check (`--locked`).
  - `solidity` (Foundry `v1.8.6`): `forge fmt --check`, `forge build --locked --sizes` (`foundry.lock`, `EIP-170`), `forge lint --deny warnings`, `forge test -vvv --gas-report` on a fresh `contracts/snapshots/` (the report is uploaded as the `gas-report` artifact), the gas snapshot check, and `script/anvil-settle.sh`.
  - `sp1` (`needs: sash`, 90-minute timeout; SP1 `v6.10.0` via the pinned `sp1up`, `protoc` from apt): `sp1/scripts/vendor-sash.sh`, the no-prover check of nitroo's `stwo` and `stwo-constraint-framework` and of SASH's `zk-prover` library, then in `sp1/`, `cargo fmt -- --check`, `cargo test --locked --workspace` (`sash-segment-check`'s unit tests included); downloads the `sash` job's `sash-synthetic-corpus` artifact into `sp1/testdata/synthetic/` and checks it against `sp1/testdata/synthetic.sha256`; runs the segment guest's executor suite on it (`cargo test --release --locked -p sp1-host --test segment_guest -- --ignored --test-threads=2`, with `SASH_SYNTHETIC_CORPUS`); then `cargo test --locked -p sp1-host --features profiling` and `cargo clippy --locked --workspace --all-targets -- -D warnings` (and for `sp1-host` with `--features profiling`); then `sp1/scripts/docker-guests.sh` (the Docker builds of both guests, their ELF sha256 and the segment guest's vkey, compared with the committed ones), and uploads the Docker-built segment guest ELF as the `segment-guest-elf` artifact (30-day retention).
  - `sash` (CUDA 12.9 toolkit from NVIDIA's apt repo, no GPU; SASH's `nightly-2025-07-14` and the repository's toolchain; sccache as in upstream's CI): `sp1/scripts/sash-synthetic.sh` (which ends with `sash-segment-check`'s bundle and differential tests), then uploads `sp1/testdata/` (corpus and records) as the `sash-synthetic-corpus` artifact.
  - **`ci.yml` spells out its steps; it does not call `make`.** A gate added to `make check` must be added to `ci.yml` too, and the other way round.

## 9. Discrepancies to confirm

None open. Record any disagreement between the code and `SPEC.md` here (see the header).
