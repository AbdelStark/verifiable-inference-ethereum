.PHONY: help check rust-check vectors forge-test anvil-demo bundles-check clean

help:
	@echo "Targets:"
	@echo "  make check       - Run Rust format, clippy, unit/negative tests, vector sync, and Foundry format, lint, tests and anvil settlement (if forge is installed)"
	@echo "  make rust-check  - Run cargo fmt --check, then clippy -D warnings and cargo test --workspace with --locked"
	@echo "  make vectors     - Regenerate contracts/src/sash/TinyLlamaFixture.sol from the Rust TinyLlama fixture"
	@echo "  make forge-test  - Check Solidity format, build against foundry.lock, lint, run the Foundry suite with gas report, and check the gas snapshots against the index"
	@echo "  make anvil-demo  - Mine the dev-mode TinyLlama-1.1B 23-segment settlement (DeployDevAnvil.s.sol) on a local Osaka anvil node, check the receipts and print their gas"
	@echo "  make bundles-check"
	@echo "                  - Check the 69 TinyLlama proof bundle files in BUNDLES (default runs/tinyllama) against sp1/pins/tinyllama.sha256"
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

anvil-demo:
	contracts/script/anvil-settle.sh

BUNDLES ?= runs/tinyllama

bundles-check:
	@test "$$(ls "$(BUNDLES)" | wc -l | tr -d ' ')" = 69 || { echo "ERROR: $(BUNDLES) must hold exactly the 69 pinned bundle files"; exit 1; }
	cd "$(BUNDLES)" && shasum -a 256 --strict --quiet -c "$(CURDIR)/sp1/pins/tinyllama.sha256"
	@echo "OK: the 69 bundle files in $(BUNDLES) match sp1/pins/tinyllama.sha256"

clean:
	cargo clean
	rm -rf contracts/out contracts/cache
