#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$root"
[[ "$(node --version)" == "v$(cat examples/browser/.nvmrc)" ]] || { echo 'Select the Node version in examples/browser/.nvmrc' >&2; exit 1; }
cargo test --locked -p ic-wire --features typescript --test runtime --test borrowed --test native
cargo clippy --locked -p ic-wire --features typescript --all-targets -- -D warnings
RUSTUP_TOOLCHAIN=1.91.0 cargo check --locked -p ic-wire --features typescript --lib
out="$root/target/typescript-conformance"
cargo run --locked -p ic-wire --features typescript --example typescript-conformance -- "$out"
"$root/examples/browser/node_modules/.bin/tsc" --ignoreConfig --strict --noUncheckedIndexedAccess --exactOptionalPropertyTypes --target ES2022 --module NodeNext --moduleResolution NodeNext --skipLibCheck --outDir "$out/compiled" "$out"/*.ts
node "$out/checks.mjs"
node crates/ic-wire/examples/typescript_conformance/endian.mjs "$out/compiled/wire-runtime.js"
bash "$root/scripts/ci/check-schema-targets.sh"

bash "$root/scripts/ci/check-bulk-advice.sh"
