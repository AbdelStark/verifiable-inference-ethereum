.PHONY: help check rust-check vectors forge-test anvil-demo clean

help:
	@echo "Targets:"
	@echo "  make check       - Run Rust format, clippy, unit/negative tests, vector sync, and Foundry format, lint and tests (if forge is installed)"
	@echo "  make rust-check  - Run cargo fmt --check, clippy -D warnings, and cargo test --workspace, all --locked"
	@echo "  make vectors     - Regenerate contracts/test/vectors/tinyllama_23seg_fixture.json from Rust primitives"
	@echo "  make forge-test  - Check Solidity format and lint, build against foundry.lock, and run Foundry test suite with gas report"
	@echo "  make anvil-demo  - Run local Anvil dev-mode deployment and TinyLlama-1.1B 23-segment settlement"
	@echo "  make clean       - Clean build outputs"

rust-check:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace

vectors:
	cargo run --locked --quiet -p sash-evm-primitives --bin export_vectors -- contracts/test/vectors/tinyllama_23seg_fixture.json

forge-test:
	cd contracts && forge fmt --check && forge lint --deny warnings && forge build --locked --sizes && forge test -vvv --gas-report

check: rust-check
	@tmp=$$(mktemp); \
	cargo run --locked --quiet -p sash-evm-primitives --bin export_vectors -- "$$tmp"; \
	cmp -s "$$tmp" contracts/test/vectors/tinyllama_23seg_fixture.json || { rm -f "$$tmp"; echo "ERROR: vector drift detected; run 'make vectors'"; exit 1; }; \
	rm -f "$$tmp"
	@if command -v forge >/dev/null 2>&1; then \
		$(MAKE) forge-test; \
	else \
		echo "NOTE: 'forge' not found in PATH; Solidity suite runs in CI or after installing Foundry."; \
	fi

anvil-demo:
	@command -v forge >/dev/null 2>&1 || { echo "Install Foundry (forge/anvil) first"; exit 1; }
	cd contracts && forge script script/DeployDevAnvil.s.sol:DeployDevAnvil -vvvv

clean:
	cargo clean
	rm -rf contracts/out contracts/cache
