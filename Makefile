.PHONY: help check rust-check vectors forge-test anvil-demo sp1-check sp1-docker clean

help:
	@echo "Targets:"
	@echo "  make check       - Run Rust format, clippy, unit/negative tests, vector sync, Foundry format, lint, tests and anvil settlement (if forge is installed), and the SP1 workspace checks (if cargo-prove is installed)"
	@echo "  make rust-check  - Run cargo fmt --check, then clippy -D warnings and cargo test --workspace with --locked"
	@echo "  make vectors     - Regenerate contracts/src/sash/TinyLlamaFixture.sol from the Rust TinyLlama fixture"
	@echo "  make forge-test  - Check Solidity format, build against foundry.lock, lint, run the Foundry suite with gas report, and check the gas snapshots against the index"
	@echo "  make anvil-demo  - Mine the dev-mode TinyLlama-1.1B 23-segment settlement (DeployDevAnvil.s.sol) on a local Osaka anvil node, check the receipts and print their gas"
	@echo "  make sp1-check   - Run cargo fmt --check, clippy -D warnings and the executor tests of the SP1 workspace in sp1/ (needs SP1 v6.10.0 and protoc)"
	@echo "  make sp1-docker  - Build the segment guest reproducibly in the pinned SP1 v6.10.0 Docker image and print its ELF sha256"
	@echo "  make clean       - Clean build outputs"

rust-check:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace

vectors:
	cargo run --locked --quiet -p sash-evm-primitives --bin export_vectors -- contracts/src/sash/TinyLlamaFixture.sol

forge-test:
	cd contracts && forge fmt --check && forge build --locked --sizes && forge lint --deny warnings
	cd contracts && rm -rf snapshots && forge test -vvv --gas-report
	@git diff --exit-code -- contracts/snapshots || { echo "ERROR: gas snapshots differ from the index; stage them if the change is intended"; exit 1; }
	@test -z "$$(git ls-files --others --exclude-standard -- contracts/snapshots)" || { echo "ERROR: untracked gas snapshots; stage them if the change is intended"; git ls-files --others --exclude-standard -- contracts/snapshots; exit 1; }

check: rust-check
	@tmp=$$(mktemp); \
	cargo run --locked --quiet -p sash-evm-primitives --bin export_vectors -- "$$tmp" || { rm -f "$$tmp"; exit 1; }; \
	cmp -s "$$tmp" contracts/src/sash/TinyLlamaFixture.sol || { rm -f "$$tmp"; echo "ERROR: TinyLlamaFixture.sol drift detected; run 'make vectors'"; exit 1; }; \
	rm -f "$$tmp"
	@if command -v forge >/dev/null 2>&1; then \
		$(MAKE) forge-test && $(MAKE) anvil-demo; \
	else \
		echo "NOTE: 'forge' not found in PATH; Solidity suite runs in CI or after installing Foundry."; \
	fi
	@if command -v cargo-prove >/dev/null 2>&1; then \
		$(MAKE) sp1-check; \
	else \
		echo "NOTE: 'cargo-prove' not found in PATH; the SP1 workspace runs in CI or after installing SP1 v6.10.0."; \
	fi
	@if command -v cargo-prove >/dev/null 2>&1 && docker image inspect $(SP1_IMAGE) >/dev/null 2>&1; then \
		$(MAKE) sp1-docker; \
	else \
		echo "NOTE: cargo-prove or the $(SP1_IMAGE) image is missing; the reproducible guest build runs in CI."; \
	fi

anvil-demo:
	contracts/script/anvil-settle.sh

SP1_IMAGE := ghcr.io/succinctlabs/sp1:v6.10.0

sp1-check:
	cd sp1 && cargo fmt -- --check
	cd sp1 && cargo clippy --locked --workspace --all-targets -- -D warnings
	cd sp1 && cargo test --locked --workspace

sp1-docker:
	cd sp1/segment-guest && cargo prove build --docker --tag v6.10.0 --locked --workspace-directory ../..
	shasum -a 256 sp1/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/segment-guest

clean:
	cargo clean
	rm -rf contracts/out contracts/cache sp1/target
