.DEFAULT_GOAL := help

override SHARED_TOOLING_ROOT := $(CURDIR)
SHARED_TOOLING_RUST := 1
export CARGO_TARGET_DIR := $(CURDIR)/target
export RUSTUP_AUTO_INSTALL := 0
LOCAL_TOOL_INSTALL_TARGETS += install-release-editor
LOCAL_TOOL_CHECK_TARGETS += release-editor-check
RELEASE_EDITOR_VERSION := 0.68.8
export CARGO_RELEASE_METADATA_MAX_BYTES := 8388608
RELEASE_EDITOR_COMMAND = bash "$(CURDIR)/scripts/dev/install-rust-tools.sh" --consumer "$(CURDIR)" \
    --package ic-build --version "$(RELEASE_EDITOR_VERSION)" --bin ic-build-release-metadata --profile debug --features cli
include make/consumer.mk

.PHONY: install-release-editor release-editor-check
install-release-editor:
	+@$(RELEASE_EDITOR_COMMAND)
release-editor-check:
	+@$(RELEASE_EDITOR_COMMAND) --check

.PHONY: help build check test lint check-wasm check-msrv check-doc-links check-pins check-tooling ci version
.PHONY: publish check-package
PUBLISH_PACKAGES ?= ic-wire-derive ic-wire
publish:
	$(if $(strip $(PUBLISH_PACKAGES)),,$(error PUBLISH_PACKAGES cannot be empty))
	$(if $(filter-out ic-wire-derive ic-wire,$(PUBLISH_PACKAGES)),$(error PUBLISH_PACKAGES must select ic-wire-derive and/or ic-wire))
	+@$(MAKE) lint
	+@env cargo publish --locked $(foreach package,$(PUBLISH_PACKAGES),-p $(package))

check-package:
	+@$(MAKE) lint
	+@env cargo package --locked --allow-dirty -p ic-wire-derive -p ic-wire --all-features --target-dir target/publication-check

.PHONY: evaluate-codecs evaluate-codec-matrix evaluate-codec-instructions check-benchmarks
evaluate-codecs:
	+@bash scripts/ci/evaluate-codecs.sh "$(CURDIR)/target/codec-comparison"

evaluate-codec-matrix:
	+@bash scripts/ci/evaluate-codec-matrix.sh "$(CURDIR)/target/codec-matrix"

evaluate-codec-instructions:
	+@env cargo run --locked -p ic-wire-benchmarks --example codec-instructions --features wire,candid,ic -- "$(CURDIR)/target/codec-comparison"

check-benchmarks:
	+@env cargo test --locked -p ic-wire-benchmarks --example codec-report
	+@env cargo test --locked -p ic-wire-benchmarks --features wire,candid --test roundtrip
	+@env cargo clippy --locked -p ic-wire-benchmarks --all-targets --features wire,candid,ic -- -D warnings
	+@env cargo clippy --locked -p ic-wire-benchmarks --bench suite --features wire,candid,allocations -- -D warnings
	+@env cargo clippy --locked -p ic-wire-benchmarks --bench suite --features wire,candid,heap -- -D warnings
	+@env RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire-benchmarks --all-targets --features wire,candid,ic
	+@env RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire-benchmarks --example codec-wasm --target wasm32-unknown-unknown --features wire,candid
	+@$(MAKE) check-consumer-benchmarks
	+@$(MAKE) check-caller-output

.PHONY: check-caller-output
check-caller-output:
	+@env cargo clippy --locked -p ic-wire-benchmarks --example caller-output-wasm --example caller-output-instructions --features caller-output,ic -- -D warnings
	+@env cargo clippy --locked -p ic-wire-benchmarks --example caller-output-wasm --example caller-output-instructions --features caller-output-baseline,ic -- -D warnings
	+@env cargo clippy --locked -p ic-wire-benchmarks --example caller-generation --features caller-generation -- -D warnings
	+@env cargo clippy --locked -p ic-wire-benchmarks --example caller-generation --features caller-generation,caller-output-baseline -- -D warnings

.PHONY: check-consumer-benchmarks
check-consumer-benchmarks:
	+@for corpus in hotspots toko derive-scale-1 derive-scale-10 derive-scale packed-i16 packed-i16-baseline packed-scale packed-scale-shared,derive-scale-10 bulk bulk,bulk-baseline bulk-scale bulk-scale,bulk-baseline; do \
	    cargo test --locked -p ic-wire-benchmarks --features "wire,candid,$$corpus" --test roundtrip && \
	    cargo clippy --locked -p ic-wire-benchmarks --all-targets --features "wire,candid,ic,$$corpus" -- -D warnings || exit $$?; \
	done
	+@for corpus in hotspots toko derive-scale packed-i16 packed-scale bulk bulk-scale; do \
	    RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire-benchmarks --all-targets --features "wire,candid,ic,$$corpus" && \
	    RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire-benchmarks --example codec-wasm --target wasm32-unknown-unknown --features "wire,candid,$$corpus" || exit $$?; \
	done

.PHONY: evaluate-allocation
evaluate-allocation:
	+@env cargo run --locked --profile evaluation -p ic-wire-qualification --example allocation --features allocation-fixture -- target/allocation-comparison evaluation

.PHONY: fuzz-runtime

fuzz-runtime:
	+@bash scripts/ci/fuzz-runtime.sh

.PHONY: check-typed-diagnostics
.PHONY: check-release check-qualification
.PHONY: check-typescript

check-typescript:
	+@bash scripts/ci/check-typescript.sh

check-qualification:
	+@env cargo test --locked -p ic-wire-qualification --lib --tests --features allocation-fixture
	+@env cargo clippy --locked -p ic-wire-qualification --all-targets --all-features -- -D warnings
	+@env RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire-qualification --all-targets --all-features

check-typed-diagnostics:
	+@bash scripts/ci/check-diagnostics.sh

help:
	@printf '%s\n' \
		'IC Wire — bounded Postcard runtime' \
		'Setup: install-tools, install-hooks; offline admission: tools-check' \
		'Rust: build, check, test, lint, check-wasm, check-msrv, fmt, fmt-check' \
		'Evaluation: check-typed-diagnostics (isolated compile-fail checks)' \
		'Evaluation: check-typescript (Rust/TypeScript runtime conformance)' \
		'Evaluation: evaluate-codecs / evaluate-codec-matrix (timing, heap, Wasm, explicit PocketIC)' \
		'Qualification: check-benchmarks (comparison corpus, lint and MSRV)' \
		'Evaluation: evaluate-allocation (borrowing, buffers and bounded/packed layouts)' \
		'Evaluation: fuzz-runtime (prepared nightly/C++/cache; FUZZ_RUNS=100000)' \
		'Qualification: check-qualification (allocation fixtures and compiler support)' \
		'Registry: publish (uploads); check-package (no upload; registry access)' \
		'Focused tooling: shared-tooling-check, check-doc-links, check-pins, check-boundary, check-tooling' \
		'Workspace counts: cloc; workspace version: version' \
		'Complete CI gate: ci (explicit local invocation)' \
		'Releases: release-patch, release-minor, release-major, release-resume (Git delivery only)'

build:
	+@env cargo build --locked -p ic-wire

check:
	+@env cargo check --locked -p ic-wire --all-targets

test:
	+@env cargo test --locked -p ic-wire

lint:
	+@env cargo clippy --locked -p ic-wire -p ic-wire-derive --all-targets -- -D warnings

check-wasm:
	+@env cargo check --locked -p ic-wire --lib --target wasm32-unknown-unknown

check-msrv:
	+@env RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire --lib
	+@env RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire --lib --target wasm32-unknown-unknown

check-doc-links:
	@perl scripts/ci/check-documentation-links.pl --root . docs/sdk-integration.md docs/evidence/sdk-output-0719/README.md docs/evidence/sdk-standard-0719/README.md
	@perl scripts/ci/check-documentation-links.pl --root . AGENTS.md README.md crates/ic-wire/README.md crates/ic-wire-derive/README.md CHANGELOG.md docs/design.md docs/evaluation.md docs/candid-comparison.md docs/full-comparison-0710.md docs/evidence/full-comparison-0710/README.md docs/full-comparison-0613.md docs/hotspot-encoding-0613.md docs/candid-consumer-comparison.md docs/scaffold-validation.md docs/development.md docs/runtime.md docs/performance.md docs/evidence/runtime-0716/README.md docs/evidence/encoder-followup-0716/README.md docs/messagepack-comparison.md docs/runtime-optimization.md docs/allocation-comparison.md docs/bounded-allocation-study.md docs/consumer-fixtures.md docs/status/current.md docs/history/candid-comparison.md docs/history/evaluation.md docs/packed-fields.md docs/unsigned-magnitude.md docs/packed-consolidation-0611.md

.PHONY: check-boundary
check-boundary:
	+@bash scripts/ci/check-cargo-dependency-boundaries.sh --consumer "$(CURDIR)" --external-ic

check-pins: host-tools-check
	+@bash scripts/ci/check-dependency-pins.sh --consumer "$(CURDIR)" --cargo-inheritance --npm-root examples/browser --node-version "$$(cat examples/browser/.nvmrc)" --npm-version "$$(jq -r '.packageManager | ltrimstr("npm@")' examples/browser/package.json)"

check-release:
	+@bash scripts/ci/test-local-release.sh

check-tooling:
	+@bash scripts/ci/test-local-make.sh

version: host-tools-check
	+@bash scripts/ci/read-cargo-workspace-version.sh --stable Cargo.toml

ci:
	+@bash scripts/ci/run-validation-targets.sh --fail-fast tools-check lint shared-tooling-check check-pins check-boundary check-doc-links check-tooling check-release fmt-check check test check-typed-diagnostics check-qualification check-benchmarks check-typescript check-wasm check-msrv

# Git release delivery is separate from package publication.
export RELEASE_DELIVERY ?= direct
export RELEASE_CACHE_PREPARE ?= 1
export RELEASE_PREVIOUS RELEASE_VERSION RELEASE_DATE RELEASE_SOURCE RELEASE_COMMIT
export RELEASE_REMOTE RELEASE_BRANCH
.PHONY: release-version release-preflight release-verify release-prepare-version release-prepared-check release-files release-commit-check release-committed-check release-tagged-check release-push-check
release-version:
	@bash scripts/ci/ic-wire-release.sh version
release-preflight:
	+@bash scripts/ci/ic-wire-release.sh preflight
release-verify:
	+@bash scripts/ci/ic-wire-release.sh verify
release-prepare-version:
	+@bash scripts/ci/ic-wire-release.sh prepare
release-prepared-check:
	+@bash scripts/ci/ic-wire-release.sh prepared
release-files:
	@bash scripts/ci/ic-wire-release.sh files
release-commit-check:
	+@bash scripts/ci/ic-wire-release.sh commit
release-committed-check:
	+@bash scripts/ci/ic-wire-release.sh committed
release-tagged-check:
	+@bash scripts/ci/ic-wire-release.sh tagged
release-push-check:
	+@bash scripts/ci/ic-wire-release.sh push
