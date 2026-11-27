# Package verification and release

The repository's product manifests and finalized changelog select **0.31.1** for
`ic-wire` and `ic-wire-derive`. Qualification packages remain unpublished. Registry
state establishes publication status; a changelog, historical approval or dry run
does not authorize or prove a new upload.

The 0.31.0 release updates Toko Tooling to 0.46.7. Platform Types 0.21.6 and
the pending qualification updates to Platform Runtime 0.23.27, Testkit 0.110.5,
Host Artifacts and FS 0.80.9, and Host Process 0.81.5 are deferred to the
separate supplier update.

The 0.30.2 qualification graph retains Platform Runtime 0.23.25. The newer
0.23.26 selection is private qualification work and can be checked separately.

The 0.30.0 release selects Platform Types 0.21.4 and Toko Tooling 0.46.3.
The private qualification graph selects Testkit 0.110.4, Platform Runtime
0.23.25, Host Artifacts and FS 0.80.8, and Host Process 0.81.4. The raw-service
and instruction checks exercised this graph on PocketIC 16.1.0.

## Planning pre-1.0 releases

Group related breaking changes that are ready together into one minor release
so consumers can update their Rust code and regenerate affected clients once.
Use a patch for compatible changes. Describe the affected surface in each
release note so consumers can tell whether a change affects their generated
client, Rust source, retained bytes or deployment. A correctness fix can ship
as soon as its affected contracts have been checked.

Before proposing 1.0, maintainers should be ready to support the frame layout
and method identity rules, public error categories and limit accounting, and
the schema metadata shared by derive, generator and runtime. Review each
against literal frames, allowance boundaries and Rust/TypeScript exchange.
Document which future changes would require a major version and how consumers
can identify them. The release decision stays with the maintainers.

## Package contract

Both product manifests select crates.io and contain package README and repository
metadata. Archive checks compare the included licence with the root licence.
The bundled TypeScript runtime is included by `include_str!` and is available to
archive consumers generating clients. No separate npm runtime is required.
Rust 1.100.0 is the supported compiler floor.

The [package guide](../crates/ic-wire/README.md) documents features, custody,
limits and TypeScript delivery. The hard cut moves `native` entrypoints to
root-level operations with `Encoding`, places optional derive under
`ic_wire::typescript`, and replaces `PackedI16<N>` with `Packed<i16, N>`.
Regenerate clients and runtime together; old runtimes do not enforce the current
generic packed bounds. Wire supplies no storage migration or fallback reader.

## Preparation checks

Run from the repository root with locked Rust dependencies already fetched:

```sh
make check-packages
# Install the private compiler if it is not already prepared:
npm ci --prefix qualification/typescript --ignore-scripts --no-audit --no-fund
# Use the client directory printed by check-packages and a prepared Chromium:
make check-browser CLIENT_DIR=/absolute/path/to/run/client BROWSER=/absolute/path/to/chrome
```

These are broad archive/browser checks for release preparation. `check-packages`
assembles both crates and extracts fresh copies under `target/`. It checks
README/licence inclusion, tests the core archive with all features, and copies
the real qualification consumer into scratch storage. The consumer checks literal
frames, metadata generation and published Platform Types identities. The script
also executes codec-only and derived Platform schema consumers on Wasm.

Only scratch manifests receive an empty `[workspace]` and local derive patch;
product source is not rewritten. This verifies archive contents without relying
on registry availability of the coordinated derive version. It is not proof that
registry consumers resolve the same packages without patches. Archive builds
resolve their scratch manifests using the caller’s Cargo settings; final
registry-dependent verification is separate.

The browser check compiles generated TypeScript to strict ES2022 modules with
rewritten `.js` imports, then executes the local fixture in the selected browser.
Its headless invocation disables Chromium's sandbox for this controlled test;
that is not an application deployment recommendation. It covers literal frames,
BigInt, UTF-8, typed arrays, selected spans, ownership, limits and malformed input.

## Recorded 0.24.4 preparation evidence

The tooling upgrade changes the packaged development dependency and lock metadata;
runtime source and normal dependency requirements remain unchanged. Fresh archive
checks passed all-feature tests, standalone consumers, Platform Types integration,
codec-only Wasm execution and all-feature Wasm forwarding. Generated clients
passed Chromium 156.0.8078.4 under `target/package-check/run-z4kZ1u/`.
Product all-feature Clippy and native driver Clippy/Rustdoc passed. The updated
Testkit 0.110.0 / Host Process 0.81.0 graph passed both installed profiles, instance
deletion and server cleanup under `qualification/target/ic-counter-p1DhNI/` before
the package version bump. These local results do not establish macOS or hosted
CI success.

## Recorded 0.24.3 preparation evidence

Fresh archive checks passed all-feature tests, Platform Types 0.21.2 consumers,
codec-only Wasm execution and all-feature Wasm forwarding. Generated clients
passed Chromium 156.0.8078.4. Evidence is under
`target/package-check/run-1JG63E/`. Relevant unchanged codec, TypeScript and
managed-server evidence from the adoption checks remains applicable; the native
run at `qualification/target/ic-counter-jqQyV7/` records its own exact inputs.

## Recorded 0.24.2 preparation evidence

The 16 November 2026 release checks passed native codec, host/derive and private
qualification tests, strict TypeScript exchange, Clippy, Rustdoc and portable
Wasm execution. Extracted archives passed all-feature tests and Platform Types
0.21.1 identity/Serde consumers. Generated clients passed Chromium 156.0.8078.4.
The migrated Runtime 0.23.11 raw query passed PocketIC 16.1.0 at opt-z and opt-3,
including nonempty-input rejection and subsequent independent literal frames.
Local evidence is retained under `target/package-check/run-42yPsI/` and
`qualification/target/ic-counter-EV9ey3/`. These passes do not establish macOS
or hosted workflow success, performance improvements or consumer rollout.

## Recorded 0.24.1 preparation evidence

The 0.24.1 release preparation recorded passing native/host/derive/qualification tests,
Clippy, strict TypeScript exchange, Rustdoc and Wasm checks. Extracted archives
passed all-feature core tests, literal-frame/platform consumers and Wasm
execution. Generated clients passed a Linux Chrome for Testing 156.0.8078.4 smoke
check. The coordinated non-publishing Cargo dry run verified both packages.
These are historical observations, not checks rerun by reading this document,
macOS proof or a Safari/Firefox matrix.

The earlier 0.23.8 offline workspace package verifier hit a temporary-registry
`no hash listed` error. The explicit extracted-archive path remains useful for
checking coordinated packages before registry publication. Online Cargo dry runs
subsequently succeeded; ordinary dry runs may need registry access even with
cached dependency sources.

[Performance qualification](design/performance.md) describes the current-only
measurement tools. Native allocation checks, Wasm size and IC counters are separate
evidence; no universal speed improvement is claimed.

## Publication procedure

Use this procedure only for a separately authorized release of the selected
version. The repository has no automatic release runner.

For a version bump, update `workspace.package.version` and the exact
`ic-wire-derive` requirement in the root `Cargo.toml` together. Match the new
requirement in [the pinning exception](../ci/dependency-pinning-exceptions.json).

1. Review final source, versions, package lists and dependency selections. Verify
   registry state and credentials at delivery time; do not treat old supplier
   freshness observations as a current review. Commit/push only when authorized.
2. Reuse unchanged focused runtime evidence and rerun checks affected by changed
   contents or dependencies. Keep final archive verification fresh.
3. Publish `ic-wire-derive` first. Once its selected version is visible in the
   registry index, verify the core archive against that actual dependency.
4. Publish `ic-wire`. Confirm both versions and build a fresh registry consumer
   without path patches. Resume partial publication from observed registry state;
   never repeat or modify a version already uploaded.

```sh
cargo publish -p ic-wire-derive --locked
# After the selected derive version is available:
cargo publish -p ic-wire --all-features --locked --dry-run
cargo publish -p ic-wire --all-features --locked
```

Consumer rollout, persisted-state operations and tag creation remain separately
scoped actions. Package verification itself performs none of them.
