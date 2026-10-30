# verifiable-inference-ethereum

Ethereum EVM and Solidity verification stack for **SASH verifiable LLM inference** ([`sg-ai-safety-hub/zk-inference-prover`](https://github.com/sg-ai-safety-hub/zk-inference-prover) at `eb95bfa887299d1637550228d42086c27cbb2b3b`, built on Circle STARKs over $M_{31}/QM_{31}$ with `Blake2sM31Channel` and mid-transcript GKR attention sumchecks).

## Why a two-layer EVM architecture is required

A 23-segment SASH run for `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`, profile `y-attnout-w12`, `S = 32` tokens: 25 prompt + 8 answer tokens) produced **761.16 MB of JSON** bundles (re-proved with SASH patch 0006, the token binding; published as the public Hugging Face dataset [`0xchqrles/sash-tinyllama-bundles`](https://huggingface.co/datasets/0xchqrles/sash-tinyllama-bundles) at commit `7284cdba1353dfd441fb4a244aabd0c9bfb9dcd5`, which `make bundles-download` fetches and checks against `sp1/pins/tinyllama.sha256`), **501,235** committed trace columns and **519,784** `QM31` OODS evaluations; verifying it runs **~5.13 million** 64-byte `BLAKE2s-256` compressions (4.90 million Merkle, 0.24 million channel). Its proofs come to **~296.60 MB** (a 289.46 MB stwo `size_estimate()` of the STARK proofs, in memory and not a wire format, plus 7.14 MB of GKR `serde_json` bytes). `sp1-host sizes` recomputes these figures from the bundles, and the M2 segment guest verifies all 23 segments in the SP1 executor in 4.79e10 cycles, with the compressions counted natively ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §1, §7); CI runs it on the published bundles.

Mainnet and Hoodi run Osaka, which binds this project; Amsterdam is tracked alongside it ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §2 lists both forks' constants). Verifying those proofs directly in one EVM transaction would need at least ~863x the `EIP-7825` per-transaction cap under Osaka (~1,131x under Amsterdam), estimated from those sizes: calldata plus ~10.27 billion gas of `BLAKE2s` in Solidity, which no precompile computes ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §3). `verifiable-inference-ethereum` therefore splits the work across two layers, without weakening SASH's execution relation:

1. **Off-chain segment verification and compression (present):**
   - M2 (`sp1/segment-guest`): an SP1 v6 `riscv64im` guest per segment runs a verifier-only build of SASH's `verify_composed` ([SPEC.md](SPEC.md) §1.1). Unmodified, `verify_composed` cannot run in a guest: it calls `std::time::Instant::now`, and its crate needs stwo's nightly `prover` feature (for the GKR verifier) and nitroo's CUDA build script.
   - M3 (`sp1/aggregator-guest`): an aggregator guest verifies the 23 segment proofs and commits `publicDigest`; an SP1 v6 BN254 PLONK wrap makes it verifiable on-chain through `Sp1SashSegmentVerifier.sol` and the pinned, ownerless SP1 v6.1.0 `SP1VerifierPlonk` ([SECURITY.md](SECURITY.md) §6), whose frame costs 286,360 gas (measured, [docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §5). Proving the TinyLlama run's 23 segments and its PLONK proof (committed as `contracts/fixtures/tinyllama.plonk.json`) takes 100.6 min and $4.61 per inference on one RTX PRO 6000 GPU rented on Hugging Face Jobs, or 18.9 min with the segments proved in parallel ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §9.4).
2. **Native Solidity SASH grid settlement (`contracts/src/`, present):**
   Enforces on-chain in pure Solidity:
   - Enrolled 23-segment `(preRoot, modelRoot, fixedHash)` manifest membership (`SashModelRegistry.sol`),
   - All 22 inter-layer residual-stream Merkle root links (`out_root[L] == in_root[L+1]`),
   - A sha256 `publicDigest` over the segment claims and the `tok_in` / `tok_out` vectors, which the segment proof must attest, so the tokens are the ones SASH's own `embed_tok` / `head_tok` checks used; those checks bind the vectors only with SASH patch 0006, which mixes them into the Fiat-Shamir transcript (upstream `eb95bfa` does not: [SECURITY.md](SECURITY.md) §5, [SPEC.md](SPEC.md) §1),
   - Public prompt prefix matching, autoregressive teacher-forcing loop closure (`tok_in[P+j] == tok_out[P-1+j]`), and EOS (`2`) termination (`SashGridVerifier.sol`, ~40k gas measured, [docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §4).

Aggregated single-transaction settlement of the TinyLlama run, with its real PLONK proof through the v6.1.0 `SP1VerifierPlonk` and `allowDevVerifier = false`, costs **520,898 gas** on Osaka (**3.1%** of the `EIP-7825` cap; anvil receipt, equal to its forge snapshot), below M3's 600,000 exit gate; the `SP1VerifierPlonk` frame is 286,360 of it. Under Amsterdam it costs 604,318 gas (provisional, mined on a Foundry nightly), 4,318 above 600,000; the gate is Osaka's ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §5).

## Exact claim boundary

An emitted `InferenceVerified` event whose `verifierKind` is not `DEV_TRANSCRIPT_ONLY` (`0`) establishes **execution integrity** for one declared run of an enrolled quantized model manifest under SASH's `y-attnout-w12` integer/lookup relation. A dev-mode event (`verifierKind == 0`) proves nothing about SASH execution. No event claims floating-point (`bf16`/`fp32`) equivalence on uncalibrated inputs, weight/prompt secrecy (zero-knowledge), or behavioral safety of the model's text output. See [SECURITY.md](SECURITY.md) and [SPEC.md](SPEC.md) for full assumption accounting.

## Quickstart

### 0. Install the pinned toolchains

| Tool | Version | How to get it |
| --- | --- | --- |
| Rust | `1.99.1` | Install [rustup](https://rustup.rs); it reads `rust-toolchain.toml` and installs `1.99.1` with `rustfmt` and `clippy` on the first `cargo` call in the repo. |
| Foundry | `v1.8.6` | Install [foundryup](https://getfoundry.sh), then `foundryup --install v1.8.6`. |
| solc | `0.8.38` | Nothing to do: `forge` downloads the version set in `contracts/foundry.toml`. |
| jq | any | Your package manager (`brew install jq`, `apt install jq`); `make anvil-demo` reads the broadcast receipts with it. |
| SP1 | `v6.10.0` | `curl -sSfL -o sp1up https://raw.githubusercontent.com/succinctlabs/sp1/v6.10.0/sp1up/sp1up && bash sp1up --version v6.10.0`; it installs `cargo-prove` into `~/.sp1/bin` and the `succinct` Rust toolchain that builds guests for `riscv64im-succinct-zkvm-elf`. Needed for the `sp1/` workspace (`make sp1-check`). |
| protoc | any recent | Your package manager (`apt install protobuf-compiler`, `brew install protobuf`), or set `PROTOC` to a [protoc release](https://github.com/protocolbuffers/protobuf/releases) binary; `sp1-sdk`'s build scripts compile protobuf definitions. |
| Docker | optional | Only for `make sp1-docker`, the reproducible guest build in the SP1 `v6.10.0` image pinned by digest in `sp1/scripts/docker-guests.sh`, on an x86_64 host. CI runs it either way. |

The Foundry dependencies (`forge-std`, `sp1-contracts`) are git submodules under `contracts/lib/`, locked in `contracts/foundry.lock`. Fetch them before any `forge` or `make` target, either by cloning with `git clone --recursive` or, in an existing clone, with:

```bash
git submodule update --init --recursive
```

### 1. Run the Rust reference grid, TinyLlama-1.1B 23-segment fixture & negative controls

```bash
make rust-check
```

This runs:
- Real 23-segment `TinyLlama-1.1B-Chat-v1.0` SASH grid verification (`22` residual Merkle links + prompt, teacher-forcing, answer and EOS checks for `"Name the capital of France in one short sentence."` -> `"The capital of France is Paris."`),
- The sha256 `manifestRootsHash`, token hashes and `publicDigest` checked against [SPEC.md](SPEC.md) §1.4,
- The grid-level negative controls (the Rust rows of the [SPEC.md](SPEC.md) §2.1 negative-control table).

### 2. Run the Foundry suite and the dev-mode settlement script

```bash
make forge-test
make anvil-demo
```

`make anvil-demo` starts `anvil --hardfork osaka --enable-tx-gas-limit` (chain `31337`; port `8577`, or `ANVIL_PORT`) and mines two runs on it from anvil's first dev account: `DeployDevAnvil.s.sol`'s five transactions (the dev verifier, `allowDevVerifier = true`), then `DeployAnvilSp1.s.sol`'s six (the v6.1.0 `SP1VerifierPlonk`, the registry and `enrollModel`, the adapter, the settlement with `allowDevVerifier = false`, and `verifyAndSettle` with the committed PLONK proof, `contracts/fixtures/tinyllama.plonk.json`; no dev verifier). For each it checks the receipts (status, `EIP-7825` gas limit, exactly one `InferenceVerified` with the `SPEC.md` §1.4 `runId` and `verifierKind` `0` or `1` (`ZKVM_PLONK`), `verifyAndSettle`'s gas against its snapshot and against the `EIP-7623` total, and its execution gas, from `trace_transaction`, against `dev_settle_execution_gas` or `sp1_settle_execution_gas` in the script), and that the SP1 settle costs less than 600,000 gas, the M3 exit gate. It prints both tables with each settle's `EIP-7623` breakdown and stops the node. `make check` runs it too.

### 3. Deploy to Hoodi (`560048`) (M4)

Testnet deployment is M4. It targets Hoodi; Sepolia (`11155111`) is optional. It disables `DevTranscriptSegmentVerifier` (`allowDevVerifier = false`) and binds `Sp1SashSegmentVerifier` immutably to the pinned SP1 v6.1.0 `SP1VerifierPlonk` (`SP1_VERIFIER`, `0xc3c6dDDAc8829b233Dc6536Ec024775a57b0AF2A` on Hoodi and Sepolia, never the `SP1VerifierGateway`: [SECURITY.md](SECURITY.md) §6) and to `SASH_AGGREGATOR_VKEY`, which needs the M3 aggregator.

```bash
cd contracts
forge script script/DeployTestnet.s.sol:DeployTestnet --rpc-url "$HOODI_RPC_URL" --broadcast --verify -vvvv
```

## Documentation

- [SPEC.md](SPEC.md): Proved statement, sha256 digest layouts, contract interfaces, and the negative-control table (§2.1).
- [SECURITY.md](SECURITY.md): Threat model, transparent-STARK vs BN254-wrapper assumption shift, and dev-verifier isolation.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): System dataflow across SASH, zkVM compression, and EVM contracts.
- [docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md): Figures from the published 23-segment TinyLlama-1.1B SASH run, the Osaka / Amsterdam network rules, measured gas and estimates.
- [docs/ROADMAP.md](docs/ROADMAP.md): Milestones `M0` through `M4`.
- [docs/STARK-NATIVE-ROUTE.md](docs/STARK-NATIVE-ROUTE.md): STARK-native routes to Ethereum compared with the SP1 BN254 wrap (research note, 2026-10-27).

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
