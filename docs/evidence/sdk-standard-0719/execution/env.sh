#!/usr/bin/env bash
export RUSTUP_HOME=/home/devbox/.rustup
export RUSTUP_TOOLCHAIN=1.99.1
export CARGO_HOME=/srv/shared/code/newstack/cdk-rsx/ver1/.cargo-home
export CARGO_TARGET_DIR=/tmp/ic-wire-standard-20261028-y6rOQ1/target
export TMPDIR=/tmp/ic-wire-standard-20261028-y6rOQ1/temp
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true RUSTUP_AUTO_INSTALL=0
export RUNS=2 SAMPLES=11 BATCH_MS=2
export PATH=/tmp/ic-wire-standard-20261028-y6rOQ1/bin:$PATH
export POCKET_IC_BIN=/srv/shared/code/newstack/playground/private-frozen-binaries/sha256/6a9a75ad9b4551d2b0f4a0d954ffb8bd5bbd0ebc44ac00290679b06ad0b2f8e6/pocket-ic
unset RUSTC RUSTDOC RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_TARGET IC_TESTKIT_POCKET_IC_URL
