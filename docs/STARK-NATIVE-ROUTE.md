# STARK-native route vs the SP1 BN254 wrap for settling SASH proofs

The question raised in review: SASH already produces Circle STARK proofs, so why wrap them in an SP1 proof, and is there a STARK-native route to Ethereum instead? This write-up records what we found on 2026-10-27, with sources. It informs M2 (SP1 guest) and M3 (wrap choice); it changes no decision by itself.

## Short answer

- **No off-the-shelf STARK-native route exists today**, for two reasons:
  - StarkWare's recursion stack doesn't accept SASH's proof format.
  - SHARP's Ethereum verifier still verifies Stone proofs.
- **A custom STARK-native route is plausible**, and it needs no re-proving:
  1. Write a verifier for SASH's current format as a Cairo program, or as a custom stwo circuit.
  2. Prove it with S-two, whose Cairo AIR has native BLAKE2s and QM31 opcodes.
  3. Submit it to SHARP as a Cairo program, through Herodotus Atlantic, which accepts Cairo PIEs only. SHARP registers a fact on L1. A custom stwo circuit gets there only through a Cairo run of `stwo_circuit_verifier`.
  
  It is transparent, with no BN254 and no trusted setup. It is also many months of work that nobody has done.
- **The SP1 route works with today's proofs**, but it is weak exactly where STARK tooling is strong: SP1 has no BLAKE2s or M31 precompile. M2's go/no-go should therefore compare it against the Cairo/S-two route.

## Why SASH proofs can't settle on Ethereum directly

The EIP facts below are verified; the sizes and gas multiples are estimates from an unpublished run (`docs/EVM-GAS-MODEL.md` §1, §3):
- **No BLAKE2s on the EVM.** The precompile at `0x09` (`EIP-152`) is only the BLAKE2b `F` compression function, over 64-bit words. SASH hashes with 32-bit BLAKE2s.
- **One segment is bigger than a block.** One segment proof (`B0L0`) is about 13.45 MB by stwo's in-memory `size_estimate()` plus its GKR bytes (its JSON bundle is 33.8 MB), above the Osaka block size limit of `EIP-7934` (about 8 MiB). Its calldata floor alone is about 28.5x the `EIP-7825` per-transaction cap.
- **All 23 segments need at least ~800x the cap** under Osaka (`docs/EVM-GAS-MODEL.md` §3). These figures are estimates from the sizes of an unpublished run.
- **Any route must recurse off-chain first.** SASH proofs have to become one small proof before they reach L1.

## SASH's proof format, at `eb95bfa`

- **Stwo fork:** a vendored NitrooZK fork of stwo v2.1.0, plus SASH's own verifier edits.
- **Merkle trees:** non-lifted, mixed-degree, hashed with M31-reduced BLAKE2s (`vcs::Blake2sM31MerkleChannel`).
- **Trees per segment:** 6 to 10, composition tree included: shape, model, base and interaction, then one per boundary side (residual stream in and out, K/V), then the composition tree (`docs/EVM-GAS-MODEL.md` §1).
- **GKR:** one LogUp-GKR batch proof per segment, covering the lookups of up to five component families (attention key tiles, dequant, residual stream, Freivalds, attnout; `head` carries one too). It is verified outside stwo's `verify`, between trees 2 and 3, by `partially_verify_batch` from stwo's `prover` module, which needs the `prover` feature and a nightly toolchain. This repo's `sp1/patches/0002-move-gkr-verifier-to-core-lookups.patch` moves `partially_verify_batch` and the sum-check verifier to `stwo::core::lookups`, so the verifier build needs neither.
- **PCS:** `log_blowup = 1`, 100 queries, 0 PoW bits, for 100 conjectured bits.

## StarkWare's stack today

- **Repository:** development moved to `starkware-libs/proving` (stwo 2.4) in July 2026.
- **Recursion verifier:** `circuits-stark-verifier` takes any constraint system through its `Statement` trait. Its proof format is fixed:
  - exactly 4 trees, all lifted to one height;
  - lifted Blake2s Merkle trees;
  - a newer transcript (salt, FRI-config mixing, PoW);
  - LogUp only, no GKR.
  
  SASH proofs don't match, and roots of different tree layouts can't be converted.
- **Cairo verifier on `main`:** checks only proofs of circuits built with StarkWare's circuit library (its gate DSL). The Cairo verifiers for Cairo CPU-AIR proofs are frozen on the `cairo-verifier-frozen` branch.
- **The L1 path:** Ethereum verifies a Stone proof of SHARP's bootloader, at about 6.2M gas across ~12 transactions per batch, shared by every task in it. S-two proves the tasks inside that recursion. StarkWare announced in March 2026 that the L1 verifier will move to S-two circuit proofs. That hasn't shipped and has no date.
- **Third-party access:**
  - Herodotus Atlantic takes Cairo PIEs (the program, memory and builtin data of a Cairo run), proves them with Stone or, if `sharpProver` selects it, S-two, and registers SHARP facts on L1. It is a paid service: credits bought under an API key, or pay-per-job via x402 from a wallet. There is no public price list.
  - Herodotus Integrity (on Starknet) handles Stone proofs only.

## Routes

| Route | Exists today | Works with today's SASH proofs | L1 cost | Trust | Verdict |
|---|---|---|---|---|---|
| **SP1 wrap** (current plan): SP1 runs SASH's verifier per segment, aggregates, BN254 Groth16 or PLONK | Verifier contracts yes; guest and aggregator no | Yes | ~270k (Groth16) to ~300k (PLONK) verify; settlement ~474k with Groth16, ~504k with PLONK (estimates, `docs/EVM-GAS-MODEL.md` §5) | BN254, setup ceremony, SP1 circuits | Current M3 plan; proving cost unmeasured |
| **Raw Solidity verifier** | No | No: block and tx limits | Billions of gas | Hash-based | Dead |
| **Cairo/circuit verifier for today's format**, S-two proof, SHARP fact | SHARP, S-two and Atlantic yes; the SASH verifier no; SHARP acceptance of a task this size untested | Yes, no re-proving | Our settlement + a fact lookup; SHARP root shared | Transparent; SHARP verifier (2/4 multisig, 8-day delay) and its unreproduced allowlist of recursive verifier programs | Most credible STARK-native route; many months |
| **Port SASH to stwo 2.4** (lifted trees, 4 trees, no GKR), StarkWare recursion, SHARP or its future L1 circuit verifier | Off-chain half yes; L1 circuit verifier no | No: re-prove TinyLlama | Unknown | Transparent | Strategic end state; depends on StarkWare and SASH upstream |
| **Own Solidity verifier** for one small recursed Keccak-channel proof | No | After recursion | Unmeasured: several million to ~20M gas (UPP's unverified claim), so possibly split across txs; unshared | Hash-based, our own unaudited code | Only if dropping BN254 is a hard requirement |
| **Starknet (Integrity) then L1** | Stone only | No | Amortised | Adds Starknet Stage 1 trust | Dominated by the SHARP route |
| **Other endings** (RISC Zero, Airbender: BN254 wraps; WHIR: hash-based research) | RISC Zero and Airbender wrap in BN254 (Groth16, FFLONK); WHIR is research | No hash-only SNARK verifier yet | WHIR PCS alone ~5.6–6.4M in software, ~4.3M with experimental precompiles | — | Watch WHIR |

## SP1's weak spots

- **No BLAKE2s or M31/QM31 precompile in SP1 v6.10.** The ~4.64M BLAKE2s compressions and the field arithmetic run as plain RISC-V. Our earlier estimate was roughly 10^10 cycles per inference. `docs/EVM-GAS-MODEL.md` §6 measures the unit costs, at 2,139 cycles per compression and 148 per DEEP-quotient step. Its extrapolation of the measured parts alone comes to ~2.3e10 cycles for all 23 segments with a `bincode` proof. OODS evaluation, FRI, the channel and GKR are not measured yet (#45). S-two Cairo and StarkWare's circuit library make exactly these operations native.
- **Large guest input.** `verify_composed` reads serde JSON: about 761 MB per run, 84.6 MB for the `head` segment alone. Decoding JSON in the guest costs ~30 cycles per byte, so the proof buffer moves to `bincode`, at 8x fewer cycles (`docs/EVM-GAS-MODEL.md` §6.2, `SPEC.md` §1.1).
- **No verifier-only build.** The verifier sits inside SASH's monolithic prover crate, which always enables stwo's `prover` and `parallel` features, rayon and the CUDA modules, and needs nightly. A guest means splitting out and maintaining a verifier crate. Prior art shows a stwo verifier compiling inside SP1 (`Okm165/stwo-verifier-sp1`), but not this fork or GKR.
- **Weak Groth16 setup.** L2BEAT rates SP1 v6.1.0's Groth16 phase-2 setup red: Succinct ran it, with 18 participants and no public call. The PLONK option rests on Aztec Ignition and is rated green. If we stay on SP1, PLONK costs ~30k more gas for a stronger setup.

## Implications for the roadmap

- **M2:** measure SP1 cycles and proving cost on one segment early. Compare against a prototype Cairo or S-two verifier of the same segment before committing to M3.
- **M3:** prefer PLONK over Groth16 for the setup, unless the gas difference matters.
- **SHARP route:**
  - bind to SHARP's own contracts, not to Atlantic's upgradeable Satellite diamond proxy, whose `isCairoFactValid(fact_hash, is_mocked)` also serves facts registered without on-chain verification. Two choices:
    - the SHARP call proxy `0x47312450B3Ac8b5b8e247a6bB6d523e7605bDb60`, which follows SHARP upgrades (2/4 SHARP Multisig, 8-day delay);
    - the immutable GPS statement verifier behind it, `0x4956bda1d23F75B988644329c5B06BD1494a72b6`, which pins one verifier version but stops receiving facts if SHARP retargets the proxy;
  - any Cairo verifier that hashes with Blake2s uses the Blake opcodes, which only S-two proves, so it needs `sharpProver=stwo`. That covers a verifier for SASH's current format and the default build of `stwo_circuit_verifier`. Only a custom circuit proved with a Poseidon252 channel can use the `poseidon252_verifier` build on Atlantic's default Stone prover.

## Open questions for StarkWare

1. Is a Cairo program that verifies SASH proofs in their current format, proved by S-two and submitted to SHARP (e.g. via Atlantic), a route you'd consider realistic? Are there size or cost limits per SHARP task?
2. What is the timeline for the L1 verifier of S-two circuit proofs, and will it accept third-party circuits?
3. Should SASH move to stwo 2.4 (lifted trees, LogUp without GKR) to fit the standard recursion, or be verified as it is?
4. Do you have numbers for recursively verifying one very wide stwo proof (20k–68k columns, trace log size up to 26, ~13–34 MB)?
5. Is there a versioning commitment for the stwo proof format that an L1 verifier could be built against?
6. Can a third party pin an immutable SHARP verifier version, rather than inherit its upgrades?

## Sources

- StarkWare, recursive circuit proving (March 2026): https://starkware.co/blog/minutes-to-seconds-efficiency-gains-with-recursive-circuit-proving/
- `starkware-libs/proving@3a396f51`:
  - `crates/stark_verifier/src/proof.rs` (`N_TRACES = 4`, lifting assert);
  - `crates/stark_verifier/src/statement.rs`;
  - `crates/stark_verifier/src/verify.rs`.
- SASH `sg-ai-safety-hub/zk-inference-prover@eb95bfa`:
  - `prover/src/util.rs` (PCS config);
  - `prover/src/common.rs` (channel, trees);
  - `nitroo/Cargo.toml` (stwo 2.1.0);
  - `nitroo/crates/stwo/src/core/pcs/verifier.rs`.
- L2BEAT, Starknet (SHARP 2/4 multisig, 8-day delay; Stage 1): https://l2beat.com/scaling/projects/starknet
- L2BEAT, stwo: https://l2beat.com/zk-catalog/stwo. It labels the L1 contract a "Stwo GPS statement verifier", but the verifier source is Stone CPU-AIR (https://etherscan.io/address/0x4956bda1d23F75B988644329c5B06BD1494a72b6#code), and the recursion roots stay on Stone (https://docs.starknet.io/learn/protocol/sharp).
- L2BEAT, SP1 Hypercube (Groth16 v6.1.0 phase-2 setup red, PLONK on Aztec Ignition green): https://l2beat.com/zk-catalog/sp1hypercube
- SP1 v6.10.0 syscalls (no BLAKE2s or M31): `succinctlabs/sp1@v6.10.0:crates/zkvm/entrypoint/src/syscalls/mod.rs`
- Atlantic contract addresses: https://docs.herodotus.cloud/atlantic-api/contract-addresses
- WHIR EVM verification: https://ethresear.ch/t/evm-verification-of-whir-over-a-31-bit-field/24902
- Herodotus Atlantic, L1 verification: https://docs.herodotus.cloud/atlantic-api/steps/l1-proof-verification
- Herodotus Integrity: https://github.com/HerodotusDev/integrity
- A stwo verifier inside SP1, prior art: https://github.com/Okm165/stwo-verifier-sp1
- EIP-7934 (block size limit): https://eips.ethereum.org/EIPS/eip-7934
- Issue #3 (M5 dropped): https://github.com/AbdelStark/verifiable-inference-ethereum/issues/3
