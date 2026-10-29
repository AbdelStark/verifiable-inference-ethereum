.PHONY: help check rust-check vectors forge-test anvil-demo sp1-vendor sp1-check sp1-docker sash-synthetic sp1-guest-suite bundles-check sp1-execute sp1-fixture-check clean

help:
	@echo "Targets:"
	@echo "  make check       - Run Rust format, clippy, unit/negative tests, vector sync, Foundry format, lint, tests and anvil settlement (if forge is installed), and the SP1 workspace checks (if cargo-prove is installed)"
	@echo "  make rust-check  - Run cargo fmt --check, then clippy -D warnings and cargo test --workspace with --locked"
	@echo "  make vectors     - Regenerate contracts/src/sash/TinyLlamaFixture.sol from the Rust TinyLlama fixture"
	@echo "  make forge-test  - Check Solidity format, build against foundry.lock, lint, run the Foundry suite with gas report, and check the gas snapshots against the index"
	@echo "  make anvil-demo  - Mine the dev-mode TinyLlama-1.1B 23-segment settlement (DeployDevAnvil.s.sol) on a local Osaka anvil node, check the receipts and print their gas"
	@echo "  make sp1-vendor  - Build sp1/vendor/sash: SASH eb95bfa from its sha256-pinned tarball plus sp1/patches"
	@echo "  make sp1-check   - Vendor SASH, check nitroo's Stwo (also with count-ops) and SASH's zk-prover library without the prover feature on stable, then cargo fmt --check, the segment check's unit tests, the executor tests and clippy -D warnings in sp1/ (needs SP1 v6.10.0 and protoc)"
	@echo "  make sp1-docker  - Build the segment guest and stwo-bench reproducibly in the SP1 v6.10.0 Docker image pinned by digest, print their ELF sha256 and the segment guest's vkey, and compare both with sp1/segment-guest.elf.sha256 and sp1/segment-guest.vkey"
	@echo "  make sash-synthetic"
	@echo "                  - Linux + CUDA 12.9 toolkit only: build upstream SASH's prover, check its pinned digests, regenerate the synthetic corpus and its oracle records, and run the segment check's bundle and differential tests on it"
	@echo "  make sp1-guest-suite"
	@echo "                  - Check the synthetic corpus in SASH_SYNTHETIC_CORPUS (default sp1/testdata/synthetic) against sp1/testdata/synthetic.sha256, run the segment guest in the SP1 executor on every bundle and bundle mutation, and render TinyLlamaFixture.sol with sp1-host fixture-from-bundles from a corpus statement carrying the TinyLlama tokens"
	@echo "  make bundles-check"
	@echo "                  - Check the 69 TinyLlama proof bundle files in BUNDLES (default runs/tinyllama) against sp1/pins/tinyllama.sha256"
	@echo "  make sp1-execute BUNDLES=runs/tinyllama OUT=<dir> [ELF=<path>] [PINS=write]"
	@echo "                  - Check the bundles, run the Docker-built segment guest (ELF, default make sp1-docker's) on all 23 TinyLlama segments in the SP1 executor, check each against native check_segment and sp1/pins/tinyllama.fixed.sha256 (PINS=write writes it), print the per-segment metrics and run the controls on modified copies in OUT"
	@echo "  make sp1-fixture-check BUNDLES=runs/tinyllama [ELF=<path>]"
	@echo "                  - Check the bundles and the Docker-built segment guest (ELF), run it on all 23 TinyLlama segments in the SP1 executor, render TinyLlamaFixture.sol from its public values with sp1-host fixture-from-bundles, and compare it byte for byte with the committed fixture"
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
		$(MAKE) sash-synthetic || exit 1; \
		if command -v cargo-prove >/dev/null 2>&1; then \
			$(MAKE) sp1-guest-suite; \
		else \
			echo "NOTE: 'cargo-prove' not found in PATH; the segment guest's executor suite runs in CI."; \
		fi; \
	else \
		echo "NOTE: the synthetic SASH corpus, and the segment guest's executor suite on it, need Linux with the CUDA 12.9 toolkit (no GPU); they run in CI."; \
	fi

anvil-demo:
	contracts/script/anvil-settle.sh

# The SP1 v6.10.0 image, by the digest sp1/scripts/docker-guests.sh pins.
SP1_IMAGE := $(shell sed -n 's/^export SP1_DOCKER_IMAGE=//p' sp1/scripts/docker-guests.sh)

sp1-vendor:
	sp1/scripts/vendor-sash.sh

# nitroo's own -Dwarnings applies to the no-prover check because it runs in nitroo/; +1.99.1 overrides
# nitroo's nightly rust-toolchain.toml. Tests run before clippy: they build the guest ELFs, which
# sp1-build skips under clippy but `include_elf!` needs.
NITROO_CHECK = cd sp1/vendor/sash/nitroo && CARGO_TARGET_DIR="$(CURDIR)/sp1/target/nitroo-check" cargo +1.99.1 check --locked --no-default-features --features std

sp1-check: sp1-vendor
	$(NITROO_CHECK) -p stwo
	$(NITROO_CHECK) -p stwo --features count-ops
	$(NITROO_CHECK) -p stwo-constraint-framework
	cd sp1/vendor/sash && CARGO_TARGET_DIR="$(CURDIR)/sp1/target/nitroo-check" cargo +1.99.1 check --locked -p zk-prover --no-default-features --lib
	cd sp1 && cargo fmt -- --check
	cd sp1 && cargo test --locked --workspace
	cd sp1 && cargo test --locked -p sp1-host --features profiling,count-ops
	cd sp1 && cargo clippy --locked --workspace --all-targets -- -D warnings
	cd sp1 && cargo clippy --locked -p sp1-host --all-targets --features profiling,count-ops -- -D warnings

sp1-docker: sp1-vendor
	sp1/scripts/docker-guests.sh

sash-synthetic:
	sp1/scripts/sash-synthetic.sh

# The segment guest's executor suite (sp1-host/tests/segment_guest.rs) and fixture-from-bundles on
# the corpus (sp1-host/tests/fixture.rs), as the sp1 and sp1-cached CI jobs run them. Release: a
# debug executor is too slow for ~1.5e9-cycle runs. One thread: the native executor keeps guest
# memory in /dev/shm (~5 GB per run), and two concurrent runs overflow it (SIGBUS).
SASH_SYNTHETIC_CORPUS ?= $(CURDIR)/sp1/testdata/synthetic

sp1-guest-suite: sp1-vendor
	cd "$(SASH_SYNTHETIC_CORPUS)" && shasum -a 256 --strict --quiet -c "$(CURDIR)/sp1/testdata/synthetic.sha256"
	cd sp1 && SASH_SYNTHETIC_CORPUS="$(SASH_SYNTHETIC_CORPUS)" cargo test --release --locked -p sp1-host --test segment_guest -- --ignored --test-threads=1
	cd sp1 && SASH_SYNTHETIC_CORPUS="$(SASH_SYNTHETIC_CORPUS)" cargo test --release --locked -p sp1-host --test fixture -- --ignored

BUNDLES ?= runs/tinyllama

bundles-check:
	@test "$$(ls "$(BUNDLES)" | wc -l | tr -d ' ')" = 69 || { echo "ERROR: $(BUNDLES) must hold exactly the 69 pinned bundle files"; exit 1; }
	cd "$(BUNDLES)" && shasum -a 256 --strict --quiet -c "$(CURDIR)/sp1/pins/tinyllama.sha256"
	@echo "OK: the 69 bundle files in $(BUNDLES) match sp1/pins/tinyllama.sha256"

# The segment guest from `make sp1-docker`; sp1-execute and sp1-fixture-check check it against the
# committed sha256 and vkey.
ELF ?= $(CURDIR)/sp1/target/elf-compilation/docker/riscv64im-succinct-zkvm-elf/release/segment-guest

sp1-execute: sp1-vendor
	$(MAKE) bundles-check BUNDLES="$(BUNDLES)"
	BUNDLES="$(BUNDLES)" OUT="$(OUT)" ELF="$(ELF)" PINS="$(PINS)" sp1/scripts/sp1-execute.sh

sp1-fixture-check: sp1-vendor
	$(MAKE) bundles-check BUNDLES="$(BUNDLES)"
	BUNDLES="$(BUNDLES)" ELF="$(ELF)" sp1/scripts/sp1-fixture-check.sh

clean:
	cargo clean
	rm -rf contracts/out contracts/cache
	@# The Docker guest build runs as root, so on Linux its output under sp1/target can only be removed from a container.
	rm -rf sp1/target 2>/dev/null || docker run --rm -v "$(CURDIR)/sp1:/sp1" --entrypoint rm $(SP1_IMAGE) -rf /sp1/target
