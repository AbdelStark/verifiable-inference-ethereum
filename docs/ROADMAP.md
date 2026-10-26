# Engineering Roadmap (`M0` to `M4`)

| Milestone | Scope | Verification environment | Exit gate |
|---|---|---|---|
| **M0: Foundation & primary-source vectors** | Rust `sash-evm-primitives`, Solidity `M31`/`CM31`/`QM31`/`Blake2s`/`Blake2sM31Channel`/`SashModelRegistry`/`SashGridVerifier`/`VerifiableInferenceSettlement`, real 23-segment TinyLlama-1.1B fixture & 20 negative controls | `cargo test --workspace` + CI Foundry | Bit-exact match on Stwo & TinyLlama `B0L0`/`head` Fiat-Shamir Horner vectors; 20/20 negative controls pass |
| **M1: Local Foundry & Anvil dev-mode settlement** | Measure exact `forge test --gas-report` per contract, run `DeployDevAnvil.s.sol` against local `anvil` (`chainid 31337`) | Foundry EVM + `anvil` | `< 350k` gas for 23-segment `SashGridVerifier`; all contracts `< 24,576` B (`EIP-170`) |
| **M2: `riscv32im` SASH guest qualification** | Compile SASH `verify_composed` (`no-cuda`, pre-enrolled `pre_root` map) into `sash-zkvm-guest`; execute all 23 TinyLlama-1.1B segments in zkVM executor | SP1 local executor | 23/23 segments pass in `< 512 MiB` guest RAM; cycle counts recorded; tampered proofs fail |
| **M3: Cryptographic Groth16 compression & Anvil settlement** | Generate real BN254 Groth16 proof(s) for TinyLlama-1.1B 23-segment run; settle via `Sp1SashSegmentVerifier` (`allowDevVerifier = false`) on local `anvil` | `anvil` with real BN254 Groth16 proof | Single-tx settlement `< 7.5M` gas (batched) or `< 600k` gas (aggregated); zero dev mocks |
| **M4: Public Ethereum testnet settlement** | Deploy contracts (`allowDevVerifier = false`) to **Sepolia** (`11155111`) or **Hoodi** (`560048`), enroll TinyLlama-1.1B manifest, submit live verification tx, verify event receipt | Ethereum Sepolia / Hoodi | Finalized testnet tx hash `< 16.77M` gas (`EIP-7825` + `EIP-7623` compliant) |

Native Stwo-in-Stwo recursion (M5) is not planned. It is reopened only if SASH proofs become consumable by StarkWare's circuit recursion, StarkWare publishes a Solidity S-two verifier, or a measured design fits `EIP-7825` (or a multi-transaction fact-registry design is accepted).
