# IC Wire

Evolvable messages over bounded Serde/Postcard, with optional schema collection,
bulk-field advice and generated TypeScript clients. The codec is `no_std` with `alloc`,
Wasm-safe and independent of IC SDKs. Rust 1.100.0 is the compiler floor and the
pinned development toolchain.

The product crates are `ic-wire` and `ic-wire-derive`, both version **0.31.1**.
Start with the [package guide](crates/ic-wire/README.md) for installation, a Rust
round trip, feature flags and TypeScript delivery.

- One `Encoding` selection covers payloads and request/response frames.
- Every message has a sparse root. Fixed positional layouts are available inside
  messages.
- Owned, reusable `Vec` and caller-slice outputs share bounded field admission.
- Explicit byte, text, list and packed fields retain consumer-selected layouts.
- Optional schema collection and derive feed one validated graph and one
  bundled TypeScript interpreter. No default features are enabled.
- Transport, authentication, retries, endpoint execution and deployment belong
  to consuming applications.

Limits cover bytes, conservative allocation charges, traversal nodes and depth;
they are not a process heap cap. Custom Serde work remains trusted. Preserve
original bytes for signing, hashing and retries because decode/re-encode may
normalize collections or accepted noncanonical encodings.

The pending evolution release changes the public byte contract. Derive `Wire`
on message types, then regenerate clients and runtime together. Persisted bytes
and hash inputs belong to their owning applications.

## Repository map

| Path | Responsibility |
| --- | --- |
| `crates/ic-wire/` | Portable codec, field adapters and optional schema tooling |
| `crates/ic-wire-derive/` | Typed message projections and optional schema metadata |
| `qualification/` | Private contract tests and standalone consumer workspace |
| `qualification/ic-wire-instructions/` | Private native simulator driver |
| `vendor/toko-tooling/` | Snapshot-owned tooling; no local edits |

## Developing Wire

Contributors can start with the [design glossary](docs/DESIGN.md#vocabulary) and
the [qualification commands](qualification/README.md) for the part they are changing.

Run commands from this repository root. Prepare the pinned Rust toolchain and
locked dependencies before running the checks:

```sh
rustup show
cargo fetch --locked
cargo fetch --locked --manifest-path qualification/Cargo.toml
make qualification-tooling-check
make install-tools
make install-hooks
```

`rust-toolchain.toml` includes rustfmt, Clippy and `wasm32-unknown-unknown`.
`make install-tools` explicitly installs the reviewed common host/Rust tools;
`make tools-check` checks them offline. Make configures their PATH. For direct
interactive use, add `.tools/host/bin` and `.tools/rust/common/bin` to PATH.

Run `make install-hooks` once per clone, including existing clones after this
setup update. It sets repository-local `core.hooksPath` to
`vendor/toko-tooling/.githooks`; it refuses to replace conflicting hooks.
The shared pre-commit hook formats the staged selection in isolation, rejects
partially staged files, and preserves unrelated working edits. It never installs
tools. `make check-formatting-hooks` qualifies that behavior against Wire's
manifests and formatting inputs. Both `make fmt` and `make fmt-check` cover both
maintained workspaces.

Choose checks for the affected contract; these commands are not a mandatory
sequence:

| Scope | Commands |
| --- | --- |
| One codec contract | `cargo test -p ic-wire --lib tests::encode_tests --locked` (replace the test name as needed) |
| Native schema checks | `make test-host`, `make lint-host`, `make doc-host` |
| Strict derive | `make test-derive`, `make lint-derive` |
| Allocation admission | `make test-allocation` |
| Rust/TypeScript exchange | `make test-typescript`, `make lint-typescript` |
| Strict generated client types and exchange | `make check-typescript` |
| Standalone Rust consumer | `make test-consumer` |
| Standalone Wasm execution | `make check-consumer-wasm` |
| Derived schema execution on Wasm | `make check-wasm-host` |
| Native instruction driver, without PocketIC | `make lint-instructions`, `make doc-instructions` |

TypeScript exchange requires Node supporting `--experimental-transform-types`.
Strict checking additionally requires the private pinned compiler:

```sh
npm ci --prefix qualification/typescript --ignore-scripts --no-audit --no-fund
make check-typescript
```

Broad targets are `make test`, `make lint`, `make doc` and `make check-wasm`
(product workspace, default features), `make test-compatibility` and
`make lint-compatibility` (qualification workspace, default features), and
`make fmt-check` (both maintained workspaces). Optional host and TypeScript
checks require the focused targets above. `make help` lists archive, browser and
measurement commands; see [qualification](qualification/README.md) for their
prerequisites and evidence limits. IC measurements additionally require explicitly
selecting PocketIC.

Toko Tooling **0.46.7** is installed through its authenticated published exporter.
`.toko-tooling.snapshot` records source identity and owned bytes.
`make snapshot-check` verifies only installed bytes; `make toko-tooling-check`
also admits product declarations and Cargo.lock. `make qualification-tooling-check`
checks all qualification members against the same snapshot.

`make deps-list` lists product packages. Add `DEPENDENCY_MANIFEST=qualification/Cargo.toml` to include qualification packages.
`make deps-update` includes both maintained workspaces by
default. Updates accept `DEPENDENCY_PACKAGE` and, with it, `DEPENDENCY_PRECISE`.
Ordinary registry libraries use compatible version requirements. Exact pins are
limited to the coordinated Wire derive/runtime pair and the Toko Tooling snapshot; [reasons](AGENTS.md#dependency-selection)
are recorded in the local overlay. Lockfiles keep ordinary checks reproducible.
Reviewed tooling updates use `make toko-tooling-preview` and
`make toko-tooling-update` after updating the Cargo selection. No automatic
release runner is installed.

The checked-in [CI workflow](.github/workflows/ci.yml) runs on pull requests,
pushes to `main` and manual dispatch. Linux and macOS run tooling admission,
non-mutating formatting, hook qualification, strict Rust checks and native product
contracts. Linux also checks the simulator driver without launching PocketIC,
executes the standalone Wasm consumer, and checks generated TypeScript with Node
24.21.0 and the locked compiler. A separate Linux job scans full Git history once
per workflow run with the snapshot-pinned Gitleaks. Failed setup and formatting diagnostics are sanitized and bounded to nine
16 KiB excerpts per phase, then retained for seven days. Attempt logs over 1 MiB are skipped. Tool payloads and raw
logs are excluded from uploads. Hosted results are asynchronous evidence; local passes do
not establish macOS or hosted success. Simulator measurements, browser provisioning,
publication and deployment remain separately selected operations.

## Design and evidence

- [Current architecture and contracts](docs/DESIGN.md)
- [Current feature scope](docs/design/parity.md)
- [Performance measurement scope](docs/design/performance.md)
- [Package/browser verification and release procedure](docs/release.md)
- [Changelog](CHANGELOG.md) and [Apache License 2.0](LICENSE.md)

Measurements apply to their recorded workloads and source hashes. Current-only
Wasm size and IC instruction runners report costs separately; correctness checks
do not establish performance.

## Licensing

Wire's first-party source in this tree, including both product crates and the
bundled TypeScript runtime, uses the [Apache License 2.0](LICENSE.md) beginning
with release 0.31.1. The published 0.31.0 archives carry their
original licence. The installed Toko Tooling baseline and vendored tools are
covered by the [Toko Tooling licence](LICENSES/toko-tooling.md). External
dependencies retain their respective licences.
