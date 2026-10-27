# verifiable-inference-ethereum

Ethereum EVM and Solidity verification stack for **SASH verifiable LLM inference** ([`sg-ai-safety-hub/zk-inference-prover`](https://github.com/sg-ai-safety-hub/zk-inference-prover) at `eb95bfa887299d1637550228d42086c27cbb2b3b`, built on Circle STARKs over $M_{31}/QM_{31}$ with `Blake2sM31Channel` and mid-transcript GKR attention sumchecks).

## Why a two-layer EVM architecture is required

A verified 23-segment SASH proof for `TinyLlama/TinyLlama-1.1B-Chat-v1.0` (`fe8a4ea1ffedaf415f4da2f062534de366a451e6`, profile `y-attnout-w12`, `S = 32` tokens: 25 prompt + 8 answer tokens) spans **761.02 MB of JSON** (**289.38 MB** packed binary STARK + **7.14 MB** binary GKR = **296.51 MB**), **501,197** committed trace columns, **519,784** `QM31` OODS evaluations, and **~4.64 million** 64-byte `BLAKE2s-256` compression blocks.

On Ethereum Mainnet, Sepolia (`11155111`), and Hoodi (`560048`):
- **`EIP-7825`** (live since Fusaka) enforces a hard consensus ceiling of **`16,777,216` gas ($2^{24}$) per transaction**.
- **`EIP-7623`** (live since Pectra) enforces a calldata floor of **`40` gas per non-zero byte** (~10.53 billion gas for 296.51 MB, or ~477.5 million gas for a single 13.45 MB layer segment `B0L0`).
- **`EIP-152`** (`0x09`) provides 64-bit `BLAKE2f` (`BLAKE2b`), not 32-bit `BLAKE2s`, making 4.64M `BLAKE2s` blocks cost >9.2 billion gas in EVM bytecode.

Direct monolithic raw verification of all 23 wide segments in one EVM transaction is therefore consensus-impossible (>1,200x above `EIP-7825`). `verifiable-inference-ethereum` solves this cleanly without weakening SASH's execution relation by splitting work across two explicit layers:

1. **Off-chain segment verification & compression (zkVM BN254 adapter):**
   Verifies SASH's unmodified `verify_composed` per segment using pre-enrolled `pre_root` manifests, and compresses segment verdicts into succinct BN254 Groth16 proof(s) (`SP1SashSegmentVerifier.sol`, ~270k gas).
2. **Native Solidity SASH grid settlement (`contracts/src/`):**
   Enforces on-chain in pure Solidity:
   - Enrolled 23-segment `(pre_root, model_root)` manifest membership (`SashModelRegistry.sol`),
   - All 22 inter-layer residual-stream Merkle root links (`out_root[L] == in_root[L+1]`),
   - A sha256 `publicDigest` over the segment claims and the `tok_in` / `tok_out` vectors, which the segment proof must attest, so the tokens are the ones SASH's own `embed_tok` / `head_tok` checks used ([SPEC.md](SPEC.md) §1),
   - Public prompt prefix matching, autoregressive teacher-forcing loop closure (`tok_in[P+j] == tok_out[P-1+j]`), and EOS (`2`) termination (`SashGridVerifier.sol`, ~22k gas, estimated).

Total single-transaction settlement gas in aggregated mode is an estimated **~402k gas** (**2.4%** of the `EIP-7825` cap).

## Exact claim boundary

An emitted `InferenceVerified` event establishes **execution integrity** for one declared run of an enrolled quantized model manifest under SASH's `y-attnout-w12` integer/lookup relation. It does **not** claim floating-point (`bf16`/`fp32`) equivalence on uncalibrated inputs, weight/prompt secrecy (zero-knowledge), or behavioral safety of the model's text output. See [SECURITY.md](SECURITY.md) and [SPEC.md](SPEC.md) for full assumption accounting.

## Quickstart

### 0. Install the pinned toolchains

| Tool | Version | How to get it |
| --- | --- | --- |
| Rust | `1.99.1` | Install [rustup](https://rustup.rs); it reads `rust-toolchain.toml` and installs `1.99.1` with `rustfmt` and `clippy` on the first `cargo` call in the repo. |
| Foundry | `v1.8.6` | Install [foundryup](https://getfoundry.sh), then `foundryup --install v1.8.6`. |
| solc | `0.8.38` | Nothing to do: `forge` downloads the version set in `contracts/foundry.toml`. |

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
- The grid-level adversarial mutation rejection tests (controls 1 to 5, 10 to 13 and 16).

### 2. Run Foundry Solidity unit, gas & local Anvil settlement tests

```bash
make forge-test
make anvil-demo
```

### 3. Deploy to Ethereum Sepolia (`11155111`) or Hoodi (`560048`)

Production testnet deployments disable `DevTranscriptSegmentVerifier` (`allowDevVerifier = false`) and bind `Sp1SashSegmentVerifier` to an immutable `SP1VerifierGateway` and `sashAggregatorVKey`:

```bash
cd contracts
forge script script/DeployTestnet.s.sol:DeployTestnet --rpc-url "$SEPOLIA_RPC_URL" --broadcast --verify -vvvv
```

## Documentation

- [SPEC.md](SPEC.md): Bit-exact protocol, wire format, contract interfaces, gas table, and negative-control table (§2.1).
- [SECURITY.md](SECURITY.md): Threat model, transparent-STARK vs BN254-wrapper assumption shift, and dev-verifier isolation.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): System dataflow across SASH, zkVM compression, and EVM contracts.
- [docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md): Empirical 23-segment TinyLlama-1.1B proof measurements and `EIP-7825` / `EIP-7623` math.
- [docs/ROADMAP.md](docs/ROADMAP.md): Milestones `M0` through `M4`.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
