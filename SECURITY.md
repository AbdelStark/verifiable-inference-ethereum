# Security Model and Assumption Boundary

`verifiable-inference-ethereum` is research and prototype infrastructure for verifying SASH LLM inference proofs on the Ethereum EVM. It has **not** undergone an external security audit and must not be used to guard high-value Mainnet funds without independent review.

## 1. What an accepted proof guarantees

When `VerifiableInferenceSettlement` accepts a run in `VerifierKind.ZKVM_PLONK` mode (never `DEV_TRANSCRIPT_ONLY`, whose events prove nothing about SASH execution), and the M2 segment guest and the M3 aggregator guest (`sp1/aggregator-guest`) meet [SPEC.md](SPEC.md) §1:
1. Every segment $i \in \{0, \dots, 22\}$ committed the exact `(preRoot[i], modelRoot[i], fixedHash[i])` enrolled in `SashModelRegistry` for `modelId`, where `fixedHash[i]` is the hash of segment $i$'s whole `Fixed` statement ([SPEC.md](SPEC.md) §1.1): its params, `sizes`, `layer`, `block` and `model`, which the verifier build checks only in part or not at all.
2. Every segment passed the segment guest's rules ([SPEC.md](SPEC.md) §1.1): the verifier build of SASH's `verify_composed` with `Verdict::accepts()` (`log_blowup = 1`, `n_queries = 100`, `pow_bits = 0`), every token id `< P`, no unbound or lax gadget, sound dequant specs, no open requantize/dequantize seam, and a statement that is the single token block `B0` with no prefix keys. Upstream `accepts()` enforces only the first of these; the rest are guest rules.
3. All 22 inter-segment residual-stream Merkle roots chain without gap (`outRoot[L] == inRoot[L+1]`), and `head` carries no dangling output boundary (`outRoot[22] == 0`).
4. `tokIn` and `tokOut` are the ids the embedding (segment 0) and the argmax (`head`) committed in their proofs, with every id `< P`: SASH with patch 0006 mixes the published vectors into the Fiat-Shamir transcript before drawing the challenge `embed_tok_ok` and `head_tok_ok` check them under (§5), and their sha256 hashes are part of the `publicDigest` the segment proof attests ([SPEC.md](SPEC.md) §1, the statement the M2 and M3 guests must meet).
5. `tokIn[0..P-1]` equals `promptIds`, `tokIn[P+j] == tokOut[P-1+j]` closes every autoregressive step $j \in [0, A-2]$, `tokOut[P-1..P+A-2]` equals `answerIds`, and `tokOut[P+A-2] == eosTokenId`.

Two assumptions come with the guest, and a third with the aggregator:
- **`pre_root` comes from enrollment.** The guest does not re-derive the shape root (tree 0); it accepts the `pre_root` the statement names, and the manifest pins it. Its correctness rests on enrollment (§3).
- **The verifier build matches upstream plus patch 0006.** The guest runs a verifier-only build of `verify_composed` at `eb95bfa`, differing from upstream only by the change classes [SPEC.md](SPEC.md) §1.1 lists, reproducible as a patch series and, from M2, checked by a differential test against upstream's verifier with patch 0006 alone. Patch 0006 is the one class that changes behaviour, on the prover's side too (§5).
- **The segment proofs reach `publicDigest` only through SP1 v6's deferred-proof recursion.** The aggregator ties each segment's public values to a proof with `verify_sp1_proof(SEGMENT_VKEY, sha256(values_i))` ([SPEC.md](SPEC.md) §1.2). Only the recursion, when proving, checks the stdin proofs against those pairs, in order, and requires each to be complete and to exit 0. The SP1 executor treats the call as a no-op, so no executor test covers that binding: binding controls B1 and B2 ([SPEC.md](SPEC.md) §2.1) test it on a GPU, by proving.

The manifest pins each segment's `fixedHash`, and `SashGridVerifier` enforces it on-chain: `fixedHash` is in each segment claim and in `manifestRootsHash` ([SPEC.md](SPEC.md) §1.3), so a proof against another `Fixed` with the same two roots fails with `ManifestRootMismatch`. The guest compares it with no pin; it commits the hash it computed.

## 2. Cryptographic assumption shift across layers

| Layer | Primitive | Security basis | Post-quantum? | Trusted setup? |
|---|---|---|---|---|
| Inner SASH segment proof | Circle STARK (SASH's vendored `nitroo` Stwo, [SPEC.md](SPEC.md) header) + GKR5 over `M31`/`QM31` with `Blake2sM31Channel` | Collision/PRF resistance of `BLAKE2s-256`; ~100 bits conjectured FRI query soundness (`100 * 1 + 0`); `QM31` Schwartz-Zippel ($\le \text{deg}/2^{124}$) | Candidate yes | None |
| Outer zkVM wrapper (`ZKVM_PLONK`: the M2 segment guest, the M3 aggregator guest and its PLONK wrap) | SP1 v6 `riscv64im` STARK, wrapped for BN254 in PLONK (circuit `v6.1.0`), verified by the pinned `SP1VerifierPlonk` (§6) | BN254 pairing hardness (~100-bit classical taxonomy), the Aztec Ignition universal SRS, SP1 v6 constraint soundness | **No** (Shor breaks BN254) | **Yes, universal:** Aztec Ignition (176 participants, public; one honest participant suffices) |
| On-chain grid & digest checks | `SashGridVerifier.sol` (grid checks, sha256 `publicDigest`) | Deterministic EVM execution by Ethereum consensus; `sha256` collision resistance for the `publicDigest` binding | Yes | None |

Effective classical soundness of the combined pipeline is bounded above by $\min(100, \lambda_{\text{zkVM}}, \lambda_{\text{BN254}}) \approx 100$ bits.

## 3. Non-guarantees

- **No zero-knowledge (privacy) guarantee:** SASH uses `LOG_BLOWUP = 1` without random trace blinding, and settlement exposes `promptIds` and `answerIds` in calldata. Do not submit confidential prompts or proprietary weights expecting cryptographic hiding.
- **Quantized relation only:** SASH proves its integer/lookup relation (`y-attnout-w12`), not IEEE-754 floating-point execution.
- **Manifest enrollment trust:** On-chain verification proves execution against the enrolled `(preRoot, modelRoot, fixedHash)` manifest. Enrollment must use the published calibration: the manifest that upstream `zk-prover model-roots --out` writes and that `zk-prover boundary-grid --verify --verify-calibration` consumes. Relying parties must verify offline that those roots were derived deterministically from the intended `model.safetensors` SHA-256 and that calibration. The segment guest does not re-derive the shape root, so each `preRoot` must be re-derived offline (`zk-prover expect-root --shape-only` or `zk-prover model-roots`) before trusting an enrolled manifest.
- **`Fixed` statements come from enrollment too.** Each enrolled `fixedHash` pins one `Fixed` ([SPEC.md](SPEC.md) §1.1). Given the 23 `Fixed` files, a relying party can check that each hashes to its enrolled `fixedHash`, which shows only that the file is the enrolled statement; that `preRoot` re-derives from the file's `params` and the published calibration, which shows only that the shape tree matches the statement; and that `modelRoot` re-derives from the intended weights. None of these shows that the `params` are the intended relation (profile `y-attnout-w12`, the model's shapes, each segment's `layer`): a statement and the roots derived from it agree with each other whatever `params` the enroller chose. The guest enforces only its own rules on them (security parameters, bound edges, sound dequant specs, a single token block). The rest holds only against a statement the relying party derives itself from the model, the profile and SASH `eb95bfa`: for TinyLlama, by re-running the recipe that regenerates the 69 bundle files, `Fixed` included, byte for byte ([SPEC.md](SPEC.md) §1.4, `sp1/pins/tinyllama.sha256`), then hashing its `Fixed` files.

## 4. `DevTranscriptSegmentVerifier` isolation

`DevTranscriptSegmentVerifier.sol` exists solely so local Foundry workflows on chain `31337` (`forge test`, and `make anvil-demo` on a local anvil node) can exercise the full 23-segment TinyLlama settlement flow in milliseconds without waiting for GPU/zkVM proof compression. Three independent defenses prevent accidental production use:
1. `DevTranscriptSegmentVerifier` checks `block.chainid == 31337` in both its constructor and `verifyAggregatedSegments`.
2. `VerifiableInferenceSettlement` stores an immutable `allowDevVerifier` boolean; `DeployTestnet.s.sol` and `DeployAnvilSp1.s.sol` set `allowDevVerifier = false`, reverting with `DevVerifierForbidden()` if a `DEV_TRANSCRIPT_ONLY` verifier is ever supplied.
3. Every `InferenceVerified` event emits `VerifierKind` explicitly so off-chain indexers distinguish dev-mode runs (`0`, `DEV_TRANSCRIPT_ONLY`) from cryptographic proofs (`1`, `ZKVM_PLONK`).

## 5. Token vectors and the Fiat-Shamir transcript (patch 0006)

**What was wrong.** Upstream SASH `eb95bfa` never mixes the published token vectors `Fixed.tok_in` and `Fixed.tok_out` into its Fiat-Shamir channel. `embed_tok_ok` and `head_tok_ok` only check that their Horner sums under the challenge `w_residual_stream` equal the claims `embed_tok` and `head_tok`, and the ids committed in the trace are fixed before `w`, but the published vectors are not. Anyone holding a valid bundle can replay the transcript to `w`, change any token positions, and solve one QM31 equation (four linear equations over M31) for four slack positions. Every id stays `< P`, `fixedHash` and the claims do not change, and the verifier accepts. Reproduced on the real TinyLlama `B0L0` (`tok_in[25]`, slack `1..4`) and `head` (`tok_out[24]`, slack `0..3`) bundles. Before the fix this let a prover:
- settle a response the model never produced, by changing answer token `tokOut[24]` together with prompt token `tokIn[25]` (loop closure holds) and placing the slack in `tokOut[0..P-2]`, which the grid never reads;
- settle one honest run again once per Horner-preserving edit of `tokOut[0..P-2]`, each with the same `promptIds` and `answerIds` but another `tokOutHash`, `publicDigest` and `runId`.

Every other `Fixed` field is pinned by `fixedHash`, and every `Claims` field is mixed (`Claims::flat`): the token vectors were the only free input.

**What is now bound.** `sp1/patches/0006-bind-token-vectors-into-transcript.patch` mixes `len(tok_in) || tok_in || len(tok_out) || tok_out`, the exact `u32` ids, after tree 2 and before the first relation draw, in the prover and in `verify_composed` alike ([SPEC.md](SPEC.md) §1.1). `w_residual_stream` is then drawn after the vectors are fixed, so a second vector with the same Horner image is a Schwartz-Zippel collision rather than a choice, and an edited vector draws another `w`. The guest-level control G9 ([SPEC.md](SPEC.md) §2.1) forges each vector under the `w` its bundle was proved with, on the synthetic corpus' `embed` and `head_argmax` bundles in CI and on the real `B0L0` and `head` in `make sp1-execute`, natively and in the SP1 executor, and requires a rejection. Those bundles are proved with the patch, so a verifier without it rejects them forged or honest: G9 does not show what an unpatched verifier accepts. On a bundle proved without the patch, the same forgery solved under upstream's `w` is accepted by an unpatched verifier. The patch is guarded on both sides: G9 on the corpus fails if the prover stops mixing the vectors (the `w` its forger finds is not the one drawn after the mix), and `token_bundles_verify` fails if the verifier stops mixing them (the patched bundles then reject at rule 6, `NotAccepted`, instead of at rule 8, `NotSingleBlock`, which no corpus bundle passes).

**The fork from upstream.** The patch changes the prover's transcript as well as the verifier's. A segment that publishes no token vector (21 of TinyLlama's 23, and every synthetic corpus bundle but `embed` and `head_argmax`) mixes nothing and proves byte for byte as upstream does. `B0L0` and `head` must be proved by a SASH prover with patch 0006: their upstream proofs reject in the guest, and their patched proofs reject under upstream's verifier. The TinyLlama bundles are proved this way ([SPEC.md](SPEC.md) §1.4); on them G9, and the same forgery solved under upstream's `w`, both reject natively and in the SP1 executor ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §7). The project follows upstream's fix when it ships.

## 6. SP1 verifier binding

`Sp1SashSegmentVerifier` verifies the aggregator's PLONK proof through one pinned SP1 verifier, never through a router:

| Field | Value |
|---|---|
| Contract | `SP1VerifierPlonk`, `contracts/lib/sp1-contracts/contracts/src/v6.1.0/SP1VerifierPlonk.sol` (sp1-contracts tag `v6.1.1`, `d3629729`, `contracts/foundry.lock`) |
| `VERSION()` | `v6.1.0`, the circuit sp1-sdk `6.10.0` proves for (`sp1-prover` `SP1_CIRCUIT_VERSION`) |
| `VERIFIER_HASH()` | `0x5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac`, the sha256 of `sp1-verifier` `6.10.0`'s `vk-artifacts/plonk_vk.bin`; its first 4 bytes are the `proofBytes` selector |
| `VK_ROOT()` | `0x002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352`, `sp1-verifier` `6.10.0`'s `VK_ROOT_BYTES` |
| Address | `0xc3c6dDDAc8829b233Dc6536Ec024775a57b0AF2A` on mainnet (`1`), Sepolia (`11155111`) and Hoodi (`560048`), one CREATE2 address (`V6_1_0_SP1_VERIFIER_PLONK` in sp1-contracts' `contracts/deployments/{1,11155111,560048}.json`) |

- **No owner and no kill switch.** The verifier has no admin and `Sp1SashSegmentVerifier` binds it immutably. The registry has an owner (§7). If v6.1.0 breaks, the adapter and the settlement are redeployed against a new verifier.
- **Never the `V6_1_0_BROKEN_*` Sepolia entries** (`deployments/11155111.json`). They were built from a stale `vk.bin` before sp1-contracts `2ac5ecb`. The broken PLONK one, `0xFaa0Ef4fC2d30bca021A982214E375cdeCFcc8f8`, returns the pinned `VERIFIER_HASH` and `VERSION` `v6.1.0` and differs only in `VK_ROOT` (`0x00995aa7753b437dfe98b936e602e002155914af1922d96e1af288578adc2251`): the address and `VK_ROOT` identify the verifier, not `VERIFIER_HASH` alone. It rejects every current proof with `InvalidVkRoot`, a liveness hole rather than a soundness one.
- **Checked at deployment.** `Sp1SashSegmentVerifier`'s constructor reads the verifier's `VERIFIER_HASH()` and `VK_ROOT()` and reverts with `WrongSp1Verifier(verifierHash, vkRoot)` unless both are the values above ([SPEC.md](SPEC.md) §2). A wrong `SP1_VERIFIER` therefore fails at deploy: the v6.1.0 `SP1VerifierGroth16` on its `VERIFIER_HASH`, the broken Sepolia PLONK entry on its `VK_ROOT`, and the gateway, which has neither getter nor a fallback, or an address without code, with a bare revert (controls 28–30, [SPEC.md](SPEC.md) §2.1). The check catches a misconfigured address, not a hostile deployer: any contract can return these two values, so the address itself stays the binding. It pins the getters rather than the bytecode because the repository compiles the vendored verifier under its own settings (solc `0.8.38`, `via_ir`), whose bytecode differs from the deployed one. Both accept the committed TinyLlama proof: the deployed one on a local fork of Hoodi, for 4,687 more gas than the repository's compile ([docs/EVM-GAS-MODEL.md](docs/EVM-GAS-MODEL.md) §5).
- **Not the `SP1VerifierGateway`.** Its owner can `addRoute` any contract reporting a fresh `VERIFIER_HASH`, which would put that owner key in the soundness trust base; on Hoodi and Sepolia the owner, `0xBaB2c2aF5b91695e65955DA60d63aD1b2aE81126`, is an EOA. Hoodi's gateways also sit at other addresses than mainnet's, and each call through a gateway costs about 6k more gas.
- **Version coupling.** A proof from an SP1 SDK on another circuit version fails against this verifier, with `WrongVerifierSelector` or `InvalidVkRoot`. An SP1 bump therefore means a new pinned verifier, new proofs and a redeploy.

## 7. Pre-deployment checklist

Every public deployment (Hoodi `560048`; Sepolia `11155111`, optional) completes this checklist. Items 1-13 are completed before the broadcast, with evidence that predates it: a commit equal to the broadcast's, a CI run on that commit, block numbers below the first deployment block. The deployment's PR (#96 for Hoodi) commits them in §8 together with its record. Item 14 follows the broadcast. A completed checklist shows that the deployed stack is the reviewed one, wired as this file describes; it adds nothing to what an accepted proof guarantees (§1): quantized execution integrity for one declared run of an enrolled manifest under `y-attnout-w12`, never output safety, alignment, floating-point equivalence or hiding (§3).

### 7.1 What a deployment trusts

- **The registry has an owner.** `SashModelRegistry.OWNER` is immutable and is the deployer (`SashModelRegistry.sol:9`, `:17-21`). It can enroll any new `modelId` with a manifest of its choice, and `setModelActive(false)` makes `getManifest`, and with it every settlement of that model, revert until the owner reactivates it; with the key lost after that, only a redeploy restores settlement. It cannot change or remove an enrolled manifest (`:35-37`), and it cannot touch a settled run. §6's "No owner" covers only the verifier and the adapter. Relying parties check the `modelId` and its enrolled manifest, not only the event.
- **`runId` is scoped to one settlement contract, not to a chain.** `settledRuns` lives in each `VerifiableInferenceSettlement`, and neither `publicDigest` nor `runId` names a chain or a contract ([SPEC.md](SPEC.md) §1.3). The same proof settles on Hoodi, on Sepolia and on any other deployment, once per deployment and prompt split: one proof yields up to 14 `runId`s, one per prompt length `19..32` ([SPEC.md](SPEC.md) §2).
- **Relying parties bind to the emitter, not to `verifierKind`.** Anyone can deploy a lookalike settlement whose adapter reports `ZKVM_PLONK`, and its `InferenceVerified` events look like the real ones. An event counts only from the recorded settlement address (§8), whose immutables are checked: `REGISTRY` and `SEGMENT_VERIFIER` the recorded registry and adapter, `ALLOW_DEV_VERIFIER == false`, and the adapter's `SP1_VERIFIER` and `SASH_AGGREGATOR_VKEY` the pinned verifier (§6) and the aggregator vkey.
- **A pending settle can be front-run by an exact copy.** The copy emits the same event, and the event names no submitter (`VerifiableInferenceSettlement.sol:20-27`): the run is settled all the same, and the original reverts with `RunAlreadySettled`. The recorder (#94) records the copy as the exit-gate transaction with `make testnet-record SETTLE_TX=<hash>`.
- **No reentrancy surface.** All three external calls in `verifyAndSettle` are STATICCALLs, made before the `settledRuns` write at `:72`: `verifierKind` at `:48` (pure), `REGISTRY.getManifest` at `:53` and `verifyAggregatedSegments` at `:66` (both view). There are no proxies, no `delegatecall`, no `selfdestruct` and no payable function.
- **The compiler.** solc `0.8.38` lists no known bug in `ethereum/solidity`'s `docs/bugs_by_version.json` (fetched 2026-10-30).

### 7.2 Slither

Slither is a gate of `make check` (`make slither`, when `slither` is installed) and of CI (the `slither` job): slither-analyzer `0.11.6` with crytic-compile `0.4.2`, through Foundry `v1.8.6` (solc `0.8.38`, `via_ir`, `osaka`, 200 runs), `slither .` in `contracts/` with `contracts/slither.config.json` (`filter_paths` `lib/|test/|script/`, `fail_on` `low`, no detector excluded). On `src/` it reports 11 contracts, 102 detectors, **0 High, 0 Medium, 0 Low, 0 Optimization and 11 Informational** findings, triaged here rather than hidden. A new finding of Low or above fails the gate; a new informational one is triaged in this table.

| Detector | Count | Where | Triage |
|---|---|---|---|
| `assembly` | 2 | `SashGridVerifier.sol:17-32`, `:34-79` | `memory-safe` `mstore` blocks; the Rust and Foundry parity tests pin the digests to [SPEC.md](SPEC.md) §1.4. Accept. |
| `cyclomatic-complexity` | 1 | `verifyGrid`, `SashGridVerifier.sol:81-150` (17) | A linear list of checks. Accept. |
| `naming-convention` | 7 | `OWNER`, `VK_ROOT()`, `SP1_VERIFIER`, `SASH_AGGREGATOR_VKEY`, `REGISTRY`, `SEGMENT_VERIFIER`, `ALLOW_DEV_VERIFIER` | `forge lint`, a `make check` gate, requires SCREAMING_SNAKE for immutables, and `VK_ROOT` is SP1's ABI. Won't fix. |
| `too-many-digits` | 1 | `TinyLlamaFixture.sol:134-320` | Generated by `make vectors`, never hand-edited. Accept. |

### 7.3 The checklist

1. **Commit.** The deployed commit is the `main` tip, with `make check` and CI green on it. The build is from a clean checkout, `forge clean && forge build --locked`: a stale `contracts/out` can hold older bytecode. Evidence: the commit and its CI run.
2. **Toolchain.** solc `0.8.38` has no known bug (§7.1), Foundry is `v1.8.6` (`forge --version`), and `forge build --locked` passes against `contracts/foundry.lock`. Evidence: the CI run of item 1.
3. **Slither.** As pinned (§7.2): 0 High, Medium or Low on `src/`, and every informational finding triaged in §7.2's table. Evidence: the `slither` job of item 1's CI run.
4. **`EIP-170` sizes.** From `forge build --sizes`, runtime: `SashModelRegistry` 1,095 B, `Sp1SashSegmentVerifier` 826 B, `VerifiableInferenceSettlement` 3,773 B, all below 24,576 B. Evidence: the `solidity` job of item 1's CI run.
5. **Privileges.** Every wiring is immutable; there is no proxy, `delegatecall`, `selfdestruct` or payable path (§7.1). The only privileged role is the registry `OWNER`, a fresh deployer key held in a Foundry keystore or on a Ledger, whose address is recorded. Evidence: the deployer address, and the source at item 1's commit.
6. **No dev verifier.** `allowDevVerifier == false` in the script and read back on chain (`ALLOW_DEV_VERIFIER()`), and no `DevTranscriptSegmentVerifier` in the broadcast. Its chain-id guards are covered by `DeploymentGuards.t.sol` and control 19 ([SPEC.md](SPEC.md) §2.1). Evidence: the script at item 1's commit and the dry run's transactions.
7. **The SP1 verifier on the target chain.** Its address, `VERSION`, `VERIFIER_HASH` and `VK_ROOT` equal §6's, and its codehash equals `SP1_VERIFIER_PLONK_CODEHASH`, pinned in `contracts/script/DeployTestnet.s.sol` (#93) and checked against the chain by `contracts/test/TestnetFork.t.sol` (#92); Sepolia's broken entry is rejected (control 30 and its Sepolia-fork form). The fork tests skip, and CI stays green, when `HOODI_RPC_URL` or `SEPOLIA_RPC_URL` is unset, so a skip is no evidence: the item needs `HoodiForkTest`'s 3 tests passed (and `SepoliaForkTest`'s 3 for a Sepolia deployment), with no `[SKIP]` and no unset-variable `::warning::`. Evidence: the `solidity` job log of item 1's CI run showing them passed at their pinned blocks, or, when the repository variables are unset, the output of `forge test --match-path test/TestnetFork.t.sol` run at item 1's commit with both URLs exported in the shell.
8. **The aggregator vkey.** The `SASH_AGGREGATOR_VKEY` deployed equals `programVKey` in `contracts/fixtures/tinyllama.plonk.json` and the `bytes32` in `sp1/aggregator-guest.vkey`. Evidence: both files at item 1's commit, and the dry run's adapter.
9. **Replay.** Control 20 (the same run settled twice) and control F (single-field mutations) pass, and the `runId` scope (§7.1) is stated. Evidence: the `solidity` job of item 1's CI run.
10. **The script's guards** (#93): the chain id, the pinned verifier and its codehash, a sender other than `DEFAULT_SENDER`, and the post-deploy asserts. Evidence: the guard tests in item 1's CI run, and the dry run passing them.
11. **The rehearsal** (#94) passes on a fork of the target chain at its latest block with the real deployer as `--sender`: the settle at 525,585 gas on Hoodi, `gasLimit <= 16,777,216`, [SPEC.md](SPEC.md) §1.4's `runId`, `verifierKind == 1`. Its predicted addresses are in the preflight file, and the dry run's match them. `broadcast/DeployTestnet.s.sol/<chainid>/` is absent afterwards. Evidence: the preflight file (commit, fork block, predicted addresses).
12. **The fork rules.** `eth_config` right before the broadcast reports `forkId` `0x23aa1351` and `next` null on Hoodi (Osaka); anything else stops the deployment for a new plan. Evidence: the reading and its block number, in the preflight file.
13. **Keys.** No `--private-key` or `--password` on a command line, no key or RPC URL in the repository (CI reads the fork tests' URLs from repository variables), and `git status --ignored` reviewed before the broadcast. Evidence: the command run, and `git status --ignored` at item 1's commit.
14. **After the broadcast.** The on-chain readbacks of every immutable (§7.1), the runtime code of each contract against the build with its immutables masked, and verification on Sourcify and Blockscout (with explicit approval). Evidence: the record (§8) and the verification links.

## 8. Deployments

Each public deployment is recorded in `contracts/deployments/<chainid>.json`, written by `contracts/script/testnet-settle.sh record` (#94), with its checklist (§7.3) in a table here. No deployment is recorded yet.

**Hoodi (`560048`):** not deployed.

| Item | Evidence | Commit or block |
|---|---|---|
| 1. Commit | — | — |
| 2. Toolchain | — | — |
| 3. Slither | — | — |
| 4. `EIP-170` sizes | — | — |
| 5. Privileges | — | — |
| 6. No dev verifier | — | — |
| 7. SP1 verifier | — | — |
| 8. Aggregator vkey | — | — |
| 9. Replay | — | — |
| 10. Script guards | — | — |
| 11. Rehearsal | — | — |
| 12. Fork rules | — | — |
| 13. Keys | — | — |
| 14. After the broadcast | — | — |

**Sepolia (`11155111`, optional):** not deployed; a deployment adds the same table.
