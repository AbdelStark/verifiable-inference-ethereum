.PHONY: help check rust-check vectors forge-test anvil-demo sp1-vendor sp1-check sp1-docker sash-synthetic bundles-check clean

help:
	@echo "Targets:"
	@echo "  make check       - Run Rust format, clippy, unit/negative tests, vector sync, Foundry format, lint, tests and anvil settlement (if forge is installed), and the SP1 workspace checks (if cargo-prove is installed)"
	@echo "  make rust-check  - Run cargo fmt --check, then clippy -D warnings and cargo test --workspace with --locked"
	@echo "  make vectors     - Regenerate contracts/src/sash/TinyLlamaFixture.sol from the Rust TinyLlama fixture"
	@echo "  make forge-test  - Check Solidity format, build against foundry.lock, lint, run the Foundry suite with gas report, and check the gas snapshots against the index"
	@echo "  make anvil-demo  - Mine the dev-mode TinyLlama-1.1B 23-segment settlement (DeployDevAnvil.s.sol) on a local Osaka anvil node, check the receipts and print their gas"
	@echo "  make sp1-vendor  - Build sp1/vendor/sash: SASH eb95bfa from its sha256-pinned tarball plus sp1/patches"
	@echo "  make sp1-check   - Vendor SASH, check nitroo's Stwo without the prover feature on stable, then cargo fmt --check, the executor tests and clippy -D warnings in sp1/ (needs SP1 v6.10.0 and protoc)"
	@echo "  make sp1-docker  - Build the segment guest and stwo-bench reproducibly in the pinned SP1 v6.10.0 Docker image and print their ELF sha256"
	@echo "  make sash-synthetic"
	@echo "                  - Linux + CUDA 12.9 toolkit only: build upstream SASH's prover, check its pinned digests, and regenerate the synthetic corpus and its oracle records"
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
	@if command -v cargo-prove >/dev/null 2>&1; then \
		$(MAKE) sp1-check; \
	else \
		echo "NOTE: 'cargo-prove' not found in PATH; the SP1 workspace runs in CI or after installing SP1 v6.10.0."; \
	fi
	@# sp1-build runs the image as linux/amd64, which hangs under emulation on arm64 hosts.
	@if [ "$$(uname -m)" = x86_64 ] && command -v cargo-prove >/dev/null 2>&1 && docker image inspect $(SP1_IMAGE) >/dev/null 2>&1; then \
		$(MAKE) sp1-docker; \
	else \
		echo "NOTE: the reproducible guest build needs an x86_64 host with cargo-prove and the $(SP1_IMAGE) image; it runs in CI."; \
	fi
	@if [ "$$(uname -s)" = Linux ] && command -v nvcc >/dev/null 2>&1 && [ -e /usr/local/cuda/lib64/libcudart.so ]; then \
		$(MAKE) sash-synthetic; \
	else \
		echo "NOTE: the synthetic SASH corpus needs Linux with the CUDA 12.9 toolkit (no GPU); it runs in CI."; \
	fi

anvil-demo:
	contracts/script/anvil-settle.sh

SP1_IMAGE := ghcr.io/succinctlabs/sp1:v6.10.0

sp1-vendor:
	sp1/scripts/vendor-sash.sh

# nitroo's own -Dwarnings applies to the no-prover check because it runs in nitroo/; +1.99.1 overrides
# nitroo's nightly rust-toolchain.toml. Tests run before clippy: they build the guest ELFs, which
# sp1-build skips under clippy but `include_elf!` needs.
NITROO_CHECK = cd sp1/vendor/sash/nitroo && CARGO_TARGET_DIR="$(CURDIR)/sp1/target/nitroo-check" cargo +1.99.1 check --locked --no-default-features --features std

sp1-check: sp1-vendor
	$(NITROO_CHECK) -p stwo
	$(NITROO_CHECK) -p stwo-constraint-framework
	cd sp1 && cargo fmt -- --check
	cd sp1 && cargo test --locked --workspace
	cd sp1 && cargo test --locked -p sp1-host --features profiling
	cd sp1 && cargo clippy --locked --workspace --all-targets -- -D warnings
	cd sp1 && cargo clippy --locked -p sp1-host --all-targets --features profiling -- -D warnings

sp1-docker: sp1-vendor
	cd sp1/segment-guest && cargo prove build --docker --tag v6.10.0 --locked --workspace-directory ../..
	cd sp1/stwo-bench && cargo prove build --docker --tag v6.10.0 --locked --workspace-directory ../..
	shasum -a 256 sp1/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/segment-guest sp1/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/stwo-bench

sash-synthetic:
	sp1/scripts/sash-synthetic.sh

BUNDLES ?= runs/tinyllama

bundles-check:
	@test "$$(ls "$(BUNDLES)" | wc -l | tr -d ' ')" = 69 || { echo "ERROR: $(BUNDLES) must hold exactly the 69 pinned bundle files"; exit 1; }
	cd "$(BUNDLES)" && shasum -a 256 --strict --quiet -c "$(CURDIR)/sp1/pins/tinyllama.sha256"
	@echo "OK: the 69 bundle files in $(BUNDLES) match sp1/pins/tinyllama.sha256"

clean:
	cargo clean
	rm -rf contracts/out contracts/cache
	@# The Docker guest build runs as root, so on Linux its output under sp1/target can only be removed from a container.
	rm -rf sp1/target 2>/dev/null || docker run --rm -v "$(CURDIR)/sp1:/sp1" --entrypoint rm $(SP1_IMAGE) -rf /sp1/target
