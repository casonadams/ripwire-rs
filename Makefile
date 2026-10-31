.DEFAULT_GOAL := help

CARGO ?= cargo
CPUS ?= $(shell getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
export CARGO_BUILD_JOBS ?= $(CPUS)
export RUST_TEST_THREADS ?= $(CPUS)
export RUSTFLAGS ?= -D warnings

.PHONY: help
help: ## Display this help screen
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-16s\033[0m %s\n", $$1, $$2}'

.PHONY: ensure-grammars
ensure-grammars:
	@if [ ! -d "vendor/grammars" ] || [ -z "$$(ls -A vendor/grammars 2>/dev/null)" ]; then \
		echo "vendor/grammars missing, running update-grammars..."; \
		./scripts/update-grammars.sh; \
	fi

.PHONY: ci
ci: fmt-check clippy complexity registry crap ## Deterministic checks CI runs: format, clippy, complexity, registry, CRAP/tests (no upstream binary, no git history, no machine state)

.PHONY: all
all: fmt ci quality size resolution docs-check parity ## Run every check: format, the deterministic CI set, quality regressions against git HEAD, default answer sizes, call-resolution ceilings, generated reference freshness, and upstream parity

.PHONY: build
build: ## Build the project in debug mode
	@$(MAKE) ensure-grammars
	$(CARGO) build

.PHONY: build-release
build-release: ## Build the project in release mode
	@$(MAKE) ensure-grammars
	$(CARGO) build --release

.PHONY: check
check: ## Type check all targets
	@$(MAKE) ensure-grammars
	$(CARGO) check --workspace --all-targets

.PHONY: fmt
fmt: ## Format all Rust source files
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check formatting of Rust source files
	$(CARGO) fmt --all -- --check

.PHONY: clippy
clippy: ## Run Clippy with warnings treated as errors
	@$(MAKE) ensure-grammars
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: clippy-fix
clippy-fix: ## Automatically fix Clippy suggestions where possible
	$(CARGO) clippy --workspace --all-targets --fix --allow-dirty --allow-staged

.PHONY: clean
clean: ## Clean cargo build artifacts
	$(CARGO) clean

.PHONY: coverage
coverage: ## Generate LCOV and nextest JUnit reports with quiet test output
	@$(MAKE) ensure-grammars
	@$(CARGO) llvm-cov clean --workspace
	@$(CARGO) test --workspace --doc
	@ulimit -n 10240 2>/dev/null || ulimit -n 4096 2>/dev/null || true; \
	if $(CARGO) nextest --version >/dev/null 2>&1; then \
		NEXTEST_PROFILE="$${NEXTEST_PROFILE:-ci}" NEXTEST_SHOW_PROGRESS="$${NEXTEST_SHOW_PROGRESS:-none}" $(CARGO) llvm-cov nextest --workspace --no-report && $(CARGO) llvm-cov report --workspace --lcov --output-path target/lcov.info --ignore-filename-regex 'src/(cli|ingest|mcp)/mod\.rs$$'; \
	else \
		$(CARGO) llvm-cov --workspace --lcov --output-path target/lcov.info --ignore-filename-regex 'src/(cli|ingest|mcp)/mod\.rs$$'; \
	fi

.PHONY: crap-gate
crap-gate: ## Gate on functions exceeding CRAP threshold 30 (or emitting warnings) using an existing target/lcov.info
	@err=$$(mktemp); \
	$(CARGO) crap --path . --lcov target/lcov.info --threshold 30 --fail-above --format github >/dev/null 2>"$$err" || { cat "$$err" >&2; rm -f "$$err"; exit 1; }; \
	if [ -s "$$err" ]; then cat "$$err" >&2; rm -f "$$err"; exit 1; fi; \
	rm -f "$$err"

.PHONY: crap
crap: coverage crap-gate ## Run the tests with coverage, then gate on CRAP

.PHONY: complexity
complexity: ## Gate on cccc cognitive complexity <= 15 with GitHub error annotations
	@python3 ./scripts/check-complexity.py .

.PHONY: registry
registry: ## Check every language and command plugin directory is registered
	@python3 ./scripts/check-registry.py .

.PHONY: quality
quality: ## Measure quality regressions against git HEAD with ripwire-rs
	@if [ -x target/release/ripwire-rs ]; then \
		target/release/ripwire-rs . --quality-delta >/dev/null; \
	elif command -v ripwire-rs >/dev/null 2>&1; then \
		ripwire-rs . --quality-delta >/dev/null; \
	elif command -v ripwire >/dev/null 2>&1; then \
		ripwire . --quality-delta >/dev/null; \
	else \
		echo "Error: ripwire-rs not found"; exit 1; \
	fi

.PHONY: size
size: build-release ## Fail when a default answer outgrows its byte budget in scripts/output-budgets.toml
	@python3 ./scripts/check-output-size.py .

.PHONY: docs
docs: build-release ## Regenerate docs/REFERENCE.md (every CLI flag and MCP tool) from the binary
	@python3 ./scripts/gen-reference.py

.PHONY: docs-check
docs-check: build-release ## Fail when docs/REFERENCE.md is stale
	@python3 ./scripts/gen-reference.py --check

.PHONY: resolution
resolution: build-release ## Fail when call resolution gets worse than scripts/resolution-ceilings.toml
	@python3 ./scripts/check-resolution.py .

UPSTREAM_DIR ?= ../ripwire

.PHONY: upstream-parity
upstream-parity: build-release ## Run upstream regression.sh against target/release/ripwire-rs
	@if [ ! -d "$(UPSTREAM_DIR)" ]; then \
		echo "Error: Upstream repository not found at $(UPSTREAM_DIR)"; \
		exit 1; \
	fi
	@echo "Running upstream regression test suite..."
	@RIPWIRE_BIN="$$(pwd)/target/release/ripwire-rs" bash "$(UPSTREAM_DIR)/test/regression.sh"

REPO ?= .

.PHONY: parity
parity: build-release ## Validate upstream parity on REPO: prints ledger measurements; fails on unrecorded, stale, or invalid entries in scripts/parity-divergences.toml (default: REPO=.)
	@./scripts/validate-parity.sh "$(REPO)"

.PHONY: validate
validate: parity ## Alias for parity validation on REPO (e.g. make validate REPO=../cccc)

.PHONY: validate-parity
validate-parity: parity ## Alias for parity validation with GitHub error annotations

.PHONY: validate-knowledge
validate-knowledge: build-release ## Run deep knowledge, computed-metric, and resource comparison against upstream ripwire on REPO (default: REPO=.)
	@python3 ./scripts/validate-knowledge.py $(REPO)
.PHONY: bench
bench: build-release ## Run comprehensive speed and memory benchmark against upstream ripwire on REPO (default: REPO=.)
	@python3 ./scripts/benchmark-performance.py "$(REPO)"

.PHONY: bench-pipeline
bench-pipeline: ## Run Cargo in-memory pipeline micro-benchmarks
	@$(CARGO) bench --bench bench_pipeline
.PHONY: update-grammars
update-grammars: ## Update vendored tree-sitter grammars from upstream repositories
	@./scripts/update-grammars.sh $(GRAMMAR)
.PHONY: install-man
install-man: ## Install manual pages into candidate man directory
	@MAN_DIR=$$(test -w /opt/homebrew/share/man/man1 2>/dev/null && echo /opt/homebrew/share/man/man1 || echo ~/.local/share/man/man1); \
	mkdir -p "$$MAN_DIR"; \
	cp man/man1/ripwire-rs.1 "$$MAN_DIR/"; \
	ln -sf ripwire-rs.1 "$$MAN_DIR/rw.1"; \
	echo "Installed man pages to $$MAN_DIR"

.PHONY: install
install: build-release install-man ## Build release binary and install to cargo bin and man pages
	$(CARGO) install --path .
