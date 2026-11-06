.DEFAULT_GOAL := help

CARGO ?= cargo
CPUS ?= $(shell getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
export CARGO_BUILD_JOBS ?= $(CPUS)
export RUST_TEST_THREADS ?= $(CPUS)
export RUSTFLAGS ?= -D warnings
# Gates that run a binary (size, resolution, docs, parity, corpus-*) use the fast profile in Cargo.toml: release's
# optimizer without fat LTO, so an edit rebuilds in seconds, not minutes. Shipping and bench use release.
RELEASE_PROFILE ?= fast
BIN ?= $(or $(CARGO_TARGET_DIR),target)/$(RELEASE_PROFILE)/ripwire-rs
FILTER ?=

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
ci: fmt-check clippy complexity registry script-tests test ## Deterministic checks CI runs: format, clippy, complexity, registry, script unittests, tests (no upstream binary, no git history, no machine state)

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

.PHONY: verify-bin
verify-bin: ## Build the verification binary target/$(RELEASE_PROFILE)/ripwire-rs (fast by default: release optimizer, no fat LTO, incremental)
	@$(MAKE) ensure-grammars
	$(CARGO) build --profile $(RELEASE_PROFILE)

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

.PHONY: sweep
sweep: ## Remove stale build artifacts: cargo-sweep (unused for 14 days) when installed, else incremental (debug and fast) and llvm-cov leftovers; target/release is never touched by the fallback
	@target="$(or $(CARGO_TARGET_DIR),target)"; \
	if $(CARGO) sweep --version >/dev/null 2>&1; then \
		echo "sweep: cargo-sweep removing artifacts unused for 14 days"; \
		$(CARGO) sweep --time 14; \
	else \
		echo "sweep: cargo-sweep not installed (cargo install cargo-sweep); deleting $$target/debug/incremental, $$target/fast/incremental, $$target/llvm-cov-target and $$target/package"; \
		rm -rf "$$target/debug/incremental" "$$target/fast/incremental" "$$target/llvm-cov-target" "$$target/package"; \
	fi

.PHONY: test
test: ## Run the workspace tests: nextest ci profile (plus doc tests) when installed, else cargo test
	@$(MAKE) ensure-grammars
	@ulimit -n 10240 2>/dev/null || ulimit -n 4096 2>/dev/null || true; \
	if $(CARGO) nextest --version >/dev/null 2>&1; then \
		NEXTEST_PROFILE="$${NEXTEST_PROFILE:-ci}" NEXTEST_SHOW_PROGRESS="$${NEXTEST_SHOW_PROGRESS:-none}" $(CARGO) nextest run --workspace && $(CARGO) test --workspace --doc; \
	else \
		$(CARGO) test --workspace; \
	fi

.PHONY: t
t: ## Run the integration tests with cargo test, the fast loop (about 3 s for all 734): make t FILTER=resolve::
	@$(MAKE) ensure-grammars
	$(CARGO) test --test it $(FILTER)

.PHONY: quick
quick: ## Edit loop: cargo check --all-targets, then make t (FILTER=... narrows the tests)
	@$(MAKE) check t FILTER="$(FILTER)"

.PHONY: complexity
complexity: ## Gate on cccc cognitive complexity <= 15 with GitHub error annotations
	@python3 ./scripts/check-complexity.py .

.PHONY: registry
registry: ## Check every language and command plugin directory is registered
	@python3 ./scripts/check-registry.py .

.PHONY: script-tests
script-tests: ## Run the unittest checks for scripts/ (parity rule logic, corpus runner and verification parsing and baseline ratchet rules; no network)
	@python3 -m unittest scripts/test_validate_parity.py scripts/corpus/test_corpus.py scripts/corpus/test_verify.py

.PHONY: quality
quality: verify-bin ## Measure quality regressions against git HEAD with the verification binary (BIN, built fresh by verify-bin; never a stale release or PATH copy)
	@$(BIN) . --quality-delta >/dev/null

.PHONY: size
size: verify-bin ## Fail when a default answer outgrows its byte budget, or discloses a wrong est_tokens (needs uv for tiktoken)
	@if command -v uv >/dev/null 2>&1; then uv run --quiet --with tiktoken python3 ./scripts/check-output-size.py . --bin $(BIN); else python3 ./scripts/check-output-size.py . --bin $(BIN); fi

.PHONY: docs
docs: verify-bin ## Regenerate docs/REFERENCE.md (every CLI flag and MCP tool) from the binary
	@python3 ./scripts/gen-reference.py --bin $(BIN)

.PHONY: docs-check
docs-check: verify-bin ## Fail when docs/REFERENCE.md is stale
	@python3 ./scripts/gen-reference.py --check --bin $(BIN)

.PHONY: resolution
resolution: verify-bin ## Fail when call resolution gets worse than scripts/resolution-ceilings.toml
	@python3 ./scripts/check-resolution.py . --bin $(BIN)

CORPUS := python3 ./scripts/corpus/corpus.py
NAME ?=

.PHONY: corpus-image
corpus-image: ## Build the local SCIP indexer image (linux/amd64, needs Docker; not part of all or ci)
	docker build --platform linux/amd64 -t ripwire-corpus:local scripts/corpus

.PHONY: corpus-image-lsp
corpus-image-lsp: ## Build the local language-server image for the LSP bridge (linux/amd64, needs Docker; not part of all or ci)
	docker build --platform linux/amd64 -f scripts/corpus/Dockerfile.lsp -t ripwire-corpus-lsp:local scripts/corpus

.PHONY: corpus-image-swift
corpus-image-swift: ## Build the local Swift language-server image for the LSP bridge (linux/amd64, needs Docker; not part of all or ci)
	docker build --platform linux/amd64 -f scripts/corpus/Dockerfile.swift -t ripwire-corpus-swift:local scripts/corpus

.PHONY: corpus-index
corpus-index: ## Index the corpus repos into ~/.cache/ripwire-corpus (NAME=click for one; FORCE=1 to rebuild)
	@$(CORPUS) index $(NAME) $(if $(FORCE),--force)

.PHONY: corpus-eval
corpus-eval: verify-bin ## Score the resolver on the corpus and fail when a precision or recall floor is missed
	@$(CORPUS) eval $(NAME) --bin $(BIN)

.PHONY: corpus-measure
corpus-measure: verify-bin ## Print measured precision and recall per corpus repo with suggested floors
	@$(CORPUS) measure $(NAME) --bin $(BIN)

.PHONY: corpus-diff
corpus-diff: verify-bin ## Diff resolver verdicts on the corpus between OLD=path/to/old/ripwire-rs and the verification binary
	@test -n "$(OLD)" || { echo "usage: make corpus-diff OLD=path/to/old/ripwire-rs"; exit 1; }
	@$(CORPUS) diff "$(OLD)" $(BIN) $(NAME)

# VERIFY_LANG reads LANG only when given on the command line, never from the shell's locale variable.
VERIFY_LANG := $(if $(filter command line file,$(origin LANG)),$(LANG))
VERIFY_ARGS = --bin $(BIN) $(NAME) $(if $(VERIFY_LANG),--lang $(VERIFY_LANG))
VERIFY := $(CORPUS) verify

.PHONY: verify
verify: verify-bin ## Score every corpus entry with a cached index and the Rust self-check against baseline.toml and the floors: one table, one verdict, non-zero on a drop beyond 0.0005 or a floor miss (NAME= or LANG= narrow)
	@$(VERIFY) run $(VERIFY_ARGS)

.PHONY: verify-ratchet
verify-ratchet: verify-bin ## Record measured numbers in baseline.toml and raise corpus.toml floors to measured minus 0.002; refuses, writing nothing, when a number dropped or an entry failed
	@$(VERIFY) ratchet $(VERIFY_ARGS)

.PHONY: verify-changed
verify-changed: verify-bin ## Verify only the entries whose languages the working tree's changes touch (git diff HEAD plus untracked; LANG= or NAME= narrow)
	@$(VERIFY) changed $(VERIFY_ARGS)

.PHONY: verify-diff
verify-diff: verify-bin ## Verify, then list the sites that flipped right->wrong against OLD=path/to/old/ripwire-rs (default: the binary verify-keep-bin kept)
	@$(VERIFY) run $(VERIFY_ARGS) --diff$(if $(OLD),=$(OLD))

.PHONY: verify-keep-bin
verify-keep-bin: verify-bin ## Keep the verification binary as the baseline for verify-diff (~/.cache/ripwire-corpus/bin/baseline)
	@mkdir -p "$(HOME)/.cache/ripwire-corpus/bin" && cp "$(BIN)" "$(HOME)/.cache/ripwire-corpus/bin/baseline"
	@echo "kept $(BIN) as the verify-diff baseline"

.PHONY: verify-watch
verify-watch: ## Rebuild the fast binary on each src/ change and print only the entries whose numbers moved (LANG=dart or NAME=...; Ctrl-C stops)
	@test -n "$(VERIFY_LANG)$(NAME)" || { echo "usage: make verify-watch LANG=dart"; exit 1; }
	@$(VERIFY) watch $(VERIFY_ARGS)

.PHONY: verify-status
verify-status: ## Show stale or missing indexes, Docker and image state, and cache size (NAME= or LANG= narrow)
	@$(VERIFY) status $(NAME) $(if $(VERIFY_LANG),--lang $(VERIFY_LANG))

.PHONY: verify-warm
verify-warm: ## Build the missing corpus indexes, sequentially (needs Docker and the corpus images; NAME= or LANG= narrow)
	@$(VERIFY) warm $(NAME) $(if $(VERIFY_LANG),--lang $(VERIFY_LANG))

UPSTREAM_DIR ?= ../ripwire

.PHONY: upstream-parity
upstream-parity: verify-bin ## Run upstream regression.sh against the verification binary
	@if [ ! -d "$(UPSTREAM_DIR)" ]; then \
		echo "Error: Upstream repository not found at $(UPSTREAM_DIR)"; \
		exit 1; \
	fi
	@echo "Running upstream regression test suite..."
	@RIPWIRE_BIN="$(abspath $(BIN))" bash "$(UPSTREAM_DIR)/test/regression.sh"

REPO ?= .

.PHONY: parity
parity: verify-bin ## Validate upstream parity on REPO: prints ledger measurements; fails on unrecorded, stale, or invalid entries in scripts/parity-divergences.toml (default: REPO=.)
	@RIPWIRE_BIN="$(BIN)" ./scripts/validate-parity.sh "$(REPO)"

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
