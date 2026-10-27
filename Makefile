.PHONY: help check rust-check vectors forge-test anvil-demo clean

help:
	@echo "Targets:"
	@echo "  make check       - Run Rust format, clippy, unit/negative tests, vector sync, and Foundry format, lint and tests (if forge is installed)"
	@echo "  make rust-check  - Run cargo fmt --check, then clippy -D warnings and cargo test --workspace with --locked"
	@echo "  make vectors     - Regenerate contracts/src/sash/TinyLlamaFixture.sol from the Rust TinyLlama fixture"
	@echo "  make forge-test  - Check Solidity format, build against foundry.lock, lint, run the Foundry suite with gas report, and check the gas snapshots against the index"
	@echo "  make anvil-demo  - Run DeployDevAnvil.s.sol in Forge's in-process EVM (chain 31337): dev-mode deployment and TinyLlama-1.1B 23-segment settlement"
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
