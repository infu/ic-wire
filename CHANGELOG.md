# Changelog

## [0.32.0] - Unreleased

### ⚠️ Breaking

- Use a sparse root for every public Rust and TypeScript message, including
  explicit projections. Rebuild positional callers and regenerate clients.
- Use the normal typed encoding API for evolving messages. It manages bounded
  temporary extension storage, including when writing to a caller's slice.
- Enable `derive` for Rust macros and `typescript` for schema tooling.
  Match `Error::Incompatible` for unknown required cases or fields.

### 🔧 Changed

- Allow ordinary Serde crate paths, symmetric renames and borrow declarations on
  Wire-derived structs.
- Reduce encode instructions with checked Postcard sizing, bounded Vec writes,
  and a fast path for empty extension blocks.

### 🐛 Fixed

- Require a message-root projection for typed slice encoding.
- Validate projected layouts during TypeScript client generation. Fixed Serde
  subtrees support ordinary records and tuples.
- Reject browser byte limits that cannot fit an empty sparse root.

## [0.31.1] - 2026-11-26

### 🔧 Changed

- Both published crates now use Apache-2.0. Their archives include the full licence.
- GitHub checks contributor agreement acceptance on pull requests.

## [0.31.0] - 2026-11-26

### ➕ Added

- Opt into sparse record and tuple extensions, stable open variants, and bounded
  branch recovery from Rust declarations and generated TypeScript clients. Contributed by [infu](https://github.com/infu) in [infu/ic-wire PR #1](https://github.com/infu/ic-wire/pull/1).
- Declare typed evolving methods with `#[derive(Wire)]` on request and reply
  roots. Add optional fields to the original Rust declaration with stable IDs.
- Derive nested record and tuple evolution through common containers. Validate
  new required record fields against the deployed request and reply directions.
- Compare proposed method manifests against supported baselines in request and
  reply directions, with explicit reports for skipped data and breaking changes.
- Encode projected frames into reusable caller storage with optional row-local
  scratch preparation.

### ⚠️ Breaking

- `#[derive(Wire)]` defaults to an extensible struct or tuple. Mark ordinary
  Serde struct and tuple layouts with `#[wire(fixed)]` for schema projection.
- When matching Wire errors exhaustively, handle `Incompatible` with the Rust
  evolution feature and `'incompatible'` in generated TypeScript runtimes.

### 🔧 Changed

- Adopt Toko Tooling 0.46.7. `make deps-list` now lists product packages by default.
- Share generated serialization code between sizing and Vec output to reduce
  uncompressed Wasm size in applications that encode many distinct DTOs.

### 🧪 Testing

- Run focused sparse codec, upgrade-manifest, and strict Rust/TypeScript
  evolution checks in CI.

## [0.30.2] - 2026-11-25

### 📝 Summary

- Adopt the [Toko Studios Network Licence 1.1](https://github.com/dragginzgame/ic-wire/blob/v0.30.2/LICENSE.md) for this release. Earlier grants remain in effect.
- Update the development tooling to Toko Tooling 0.46.4 in both Cargo workspaces.

## [0.30.1] - 2026-11-24

### 🐛 Fixed

- TypeScript codec errors expose the name `WireError` so shared handlers can recognise their error codes across generated client packages. [ic-wire #16 — Set WireError.name so errors are recognisable across runtime copies](https://github.com/dragginzgame/ic-wire/issues/16)

## [0.30.0] - 2026-11-24

### ⚠️ Breaking

- Match Rust admission failures as `Error::Limit(kind)`. The kind identifies bytes, allocation allowance, nodes, depth, destination capacity or a constructor field bound. Encoded bytes and browser errors are unchanged. [ic-wire #9 — Report which allowance was exceeded in Error::Limit](https://github.com/dragginzgame/ic-wire/issues/9)

### 🔄 Changed

- Decode admitted integer, opaque and finite-float words through fixed-width chunks. The sealed word trait no longer exposes its previous single-word slice reader. [ic-wire #11 — Remove unwrap/expect panic paths in packed element reads](https://github.com/dragginzgame/ic-wire/issues/11)

### 📚 Documentation

- Work out the byte, node, depth and allocation allowances for a two-field reply. [ic-wire #10 — Make Limits easier to configure correctly](https://github.com/dragginzgame/ic-wire/issues/10)
- Document when to batch related breaking changes and what maintainers would need to support at 1.0. [ic-wire #12 — Reduce breaking-release churn before 1.0](https://github.com/dragginzgame/ic-wire/issues/12)
- Add a contributor glossary and link users directly to the package guide. [ic-wire #13 — Make docs approachable for outside contributors and users](https://github.com/dragginzgame/ic-wire/issues/13)

### 🧪 Testing

- Exercise arbitrary inputs with libFuzzer, generated values with proptest, and matching Rust and TypeScript outcomes. [ic-wire #8 — Add fuzz and property tests for untrusted-input decoding](https://github.com/dragginzgame/ic-wire/issues/8)
- Measure source lines and clean native check times for codec-only and TypeScript builds. [ic-wire #14 — Track growth of the typescript feature relative to the codec core](https://github.com/dragginzgame/ic-wire/issues/14)
- Run a generated TypeScript client over HTTP against a raw-frame query and update on PocketIC. [ic-wire #15 — Add a reference frontend↔canister integration (transport + raw-frame endpoints)](https://github.com/dragginzgame/ic-wire/issues/15)

## [0.29.0] - 2026-11-23

### ➕ Added

- Exporters can acknowledge a bulk advisory for a named list element or DTO field across an application. Each decision records a reason and the exporter can report unused decisions after its final method. [#7 — Allow a specific Wire advisory by application type or DTO field](https://github.com/dragginzgame/ic-wire/issues/7)

### ⚠️ Breaking

- `BulkWarning` exposes the advisory identifier and declaration sites. Update struct literals and exhaustive patterns for the two additional fields.

## [0.28.1] - 2026-11-23

### ➕ Added

- Generate a typed TypeScript service client from existing method codecs with
  the optional `service-client` feature. Applications supply the transport.
  [#6 — Render a typed TypeScript service client with an injected transport](https://github.com/dragginzgame/ic-wire/issues/6)

### 📦 Dependencies

- Adopt Toko Tooling 0.46.3 in the product and qualification workspaces.

## [0.28.0] - 2026-11-21

### 🔄 Changed

- Decoding a Rust `char` now requires exactly one Unicode scalar. Earlier
  releases accepted the first scalar when the declared span contained several.
  Such inputs now return `Error::Malformed`. Valid encoded characters keep
  their existing bytes.

### 📦 Dependencies

- Require Postcard 1.2.0 so codec-only native and Wasm consumers can build
  without `serde_derive`. [#1 — Adopt a published Postcard release that removes unconditional Serde derive](https://github.com/dragginzgame/ic-wire/issues/1)
- Adopt Toko Tooling 0.45.25 in both workspaces.

## [0.27.1] - 2026-11-20

### 📦 Dependencies

- Adopt Toko Tooling 0.45.23 in both workspaces.
- Inherit the product version from the workspace and record the exact derive
  dependency required by the generated metadata API. [#5 — Reconcile shared dependency-pin and inheritance audit findings](https://github.com/dragginzgame/ic-wire/issues/5)

## [0.27.0] - 2026-11-19

- Adopt Toko Tooling 0.45.22 in both workspaces. Frame encoding, decoding and
  generated client limits now use the same Rust header-size constant.

- Add the optional `api-manifest` exporter for bounded, deterministic snapshots
  of public endpoint contracts. CI checks the upload consumer against its 0.26.1
  client baseline. [IC Wire #4](https://github.com/dragginzgame/ic-wire/issues/4)

## [0.26.1] - 2026-11-19

- Adopt Toko Tooling 0.45.21 in both workspaces. Native allocation examples and
  IC instruction guests now select their entry points through Cargo targets
  and share the same numeric workloads without Wasm target guards. The
  instruction driver verifies the prepared PocketIC executable before querying
  its version.

## [0.26.0] - 2026-11-19

- The optional TypeScript schema API, Wire derive and Platform type projections
  now work on wasm32-unknown-unknown as well as native. Enabling these features
  selects their schema dependencies on both targets.

- Adopt Toko Tooling 0.45.18 and share native and Wasm entry-point wiring across
  the four numeric qualification probes.

## [0.25.2] - 2026-11-19

### ➕ Added

- Enable `derive` once in Cargo to use `#[derive(Wire)]` on shared native and
  Wasm DTOs without per-type guards. Schema generation remains host-only.

### 🧪 Testing

- Adopt Toko Tooling 0.45.16. The four numeric probes share their codec workload
  between native allocation runs and Wasm instruction measurements. Native
  examples can also run directly with Cargo.

## [0.25.1] - 2026-11-19

### 🧹 Cleanup

- Adopt Toko Tooling 0.45.14. CI now sanitizes attempt logs directly through
  the bounded reader without writing temporary copies of the raw output.
- Compile packed little-endian conversion helpers on all supported targets
  and exercise their byte conversion in native tests.

## [0.25.0] - 2026-11-18

- Bulk warnings expose the declared list bound in `max_elements` and in printed
  diagnostics. Code constructing or exhaustively matching `BulkWarning` must
  account for this field. Skip unreachable elements of empty-only collections
  and clarify the traversal and domain tradeoffs of bulk adapters.
- Integer-list advice recognizes scalar bounds that guarantee one-byte varints
  for signed and unsigned 16-, 32- and 64-bit elements.

- Adopt authenticated Toko Tooling 0.45.6 and group the product workspace's
  dependency catalog by production, shared and tooling use. Cargo checks
  respect the caller's network settings throughout CI and qualification.
- Document finite-float list prefix costs in bulk diagnostics and verify both
  the 128-byte and 16,384-byte boundaries for f32 and f64.
- Hash-list advice identifies bounds where packing reduces traversal while
  preserving the payload size for every allowed list length.
- Compare finite f32/f64 list layouts and signed/unsigned 128-bit integer lists
  with Candid, including bit preservation and non-finite adapter rejection.
- Compare sparse and dense reading layouts and matched UTF-8 byte lengths
  across both variable-length prefix boundaries exercised by the matrix.
- Compare inline and table-indexed labels across repetition rates and text widths.
- Compare fixed and variable coordinate rows, and flat and grouped relations
  across key widths and target counts, in the Wire/Candid matrix.
- Compare unit and nested optional lists with Candid, and measure absolute versus
  gap-encoded ordered integers across element-count and gap-width boundaries.
- Compare counted flag bitmaps and run-length encoded enum lists with their
  ordinary list representations in Wire and Candid. Boolean-list advice uses
  the declared bound to identify fields too short for a counted bitmap to save bytes.

## [0.24.6] - 2026-11-18

- Adopt authenticated Toko Tooling 0.45.1 and use its setup diagnostic helper in CI.
  Limit attempt-log reads to 1 MiB per file and label excerpts by number.
- Extend the private comparison with recursive trees, maps, ragged lists, results
  and bit-exact f32 values including signed zero, infinities and a NaN payload.
  Run the matrix on Linux CI with matched small, sparse-wide and wide ragged lists.
- Compare ordinary and bulk boolean and fixed-size hash fields across prefix
  boundaries, including a hash-list traversal budget regression check.
- Compare bounded List, Vec, Packed and Candid from 1 through 4,096 elements,
  and add Principal and Account round trips with exact subaccount checks.

## [0.24.5] - 2026-11-18

- Add an optional private Candid comparison for payload sizes and round trips.
- Measure numeric-list encoding, decoding, allocation and whole Wasm size across
  three value distributions. Refine bulk-field lint guidance around these costs.

## [0.24.4] - 2026-11-18

### 🔧 Dependencies

- Adopt Testkit 0.110.0 and Host Process 0.81.0 for the private instruction
  runner, and authenticated Toko Tooling 0.45.0 in both workspaces.
- Refresh the private qualification graph to Platform Runtime 0.23.17,
  Host Artifacts/FS 0.80.3, bytes 1.12.2 and zeroize 1.9.2. Retain
  Platform Types 0.21.2 and PocketIC's thiserror 2.0.18 selection.

## [0.24.3] - 2026-11-16

### 🛠️ Maintenance

- Adopt ic-testkit 0.107.3, Platform Types 0.21.2 and Runtime 0.23.14 across
  their product and qualification callers. Replace the temporary shell launcher
  with Testkit managed-server ownership and explicit lifecycle evidence. The
  product selects thiserror 2.0.21; PocketIC retains 2.0.18 in qualification.
- Adopt authenticated Toko Tooling 0.44.11, with bounded sanitized CI setup
  diagnostics and finite artifact retention. Correct the historical secret-scan
  prose suppression ([Wire #2](https://github.com/dragginzgame/ic-wire/issues/2)).
- Flatten qualification packages into one tier and one workspace/lockfile. Move
  the instruction driver to qualification/ic-wire-instructions and update
  admission, formatting, CI, archive fixtures and measurement paths while
  preserving the previous dependency selections.

## [0.24.2] - 2026-11-16

### 🔧 Dependencies

- Adopt published Platform Types 0.21.1 and Runtime 0.23.11. Migrate the private
  counter query to explicit Runtime admission and encoding; retain the raw reply
  protocol and zero-argument guard. Types owns the exact thiserror 2.0.18 selection.

### 🛠️ Maintenance

- Wire the shared formatting hook and Linux/macOS CI to the existing product,
  Wasm and TypeScript checks, with once-per-run secret scanning and explicit
  native-driver checks that do not start a simulator
  ([#2](https://github.com/dragginzgame/ic-wire/issues/2)).
- Adopt the verified Toko Tooling 0.44.6 snapshot and exact selection in all
  three workspaces, including its clarified asynchronous hosted-CI policy.
  Preserve unrelated dependency versions.

## [0.24.1] - 2026-11-15

### ▶️ Try it

From the repository root, prepare locked qualification dependencies once
(registry access required), then run the offline checks:

```sh
cargo fetch --locked --manifest-path qualification/Cargo.toml
make toko-tooling-check
cargo test --manifest-path qualification/Cargo.toml \
  -p ic-wire-compatibility --test compatibility --locked --offline
```

The tooling check admits 0.44.4; the test runs the independent codec contracts
from the renamed package directory. Neither command publishes or deploys.

### 🛠️ Maintenance

- Adopt Toko Tooling 0.44.4 through its authenticated exporter in all three
  workspaces. Rename qualification package directories to their Cargo names,
  update local callers and remove the obsolete candidate dependency alias.
- Remove the historical Wire dependency, paired comparison runners, runtime
  adapter and comparison reports. Retain independent literal, allocation,
  malformed-input and Rust/TypeScript qualification; measure only current Wasm
  and installed IC workloads.
- Allow compatible registry dependency updates while retaining coordinated derive
  and required tooling pins. Preserve surviving locked dependency versions,
  including the native graph constraints.
- Align the isolated instruction runner with the common Cargo, Rust, Rustdoc and
  Clippy policy. Document its scoped workspace exception after reproducing the
  codec dependency downgrade that merging it would cause.

### 📚 Documentation

- Replace the obsolete evaluation proposal with current architecture and API
  contracts. Refresh setup and qualification commands, remove obsolete repository links,
  and distinguish historical measurements and release preparation from current
  evidence and separately authorized publication.

## [0.24.0] - 2026-11-15

First public release of the replacement implementation, following published
0.22.4. This is a hard cut: migrate to root-level APIs with `Encoding`, replace
`PackedI16<N>` with `Packed<i16, N>`, and regenerate TypeScript clients and runtime
together. No legacy compatibility lane is retained.

The public repository is `dragginzgame/ic-wire`.

### 🐛 Fixed

- Avoid Postcard's general varint buffer for one-byte 16/32/64-bit integers
  through one shared serializer shortcut. Keep admission and exact bytes;
  request encoding now narrowly beats the reference IC instruction count in
  both profiles, with a small Wasm-size and opt-z response cost.
- Validate finite-f32 buffers two words at a time, preserving exact finite bits
  and rejection before allocation. Qualify exponent lanes, native timing and a
  separate Wasm float consumer; keep the f64 path unchanged.

### ✨ Added

- Expand the native comparison to 52 workload families, owned/borrowed decoding,
  framing failures and position-sensitive domain rejection. Add integer/length
  boundaries, collection shapes, identities and bitwise float/borrowing checks
  without importing the legacy feature-matrix or adding product dependencies.

### ▶️ Try it

Add `ic-wire = "0.24.0"` to use the portable codec; enable `derive` for host schema
projection and TypeScript generation. See the [package guide](crates/ic-wire/README.md).

## [0.23.9] - 2026-11-15

GitHub evaluation patch requested by the user; not published to crates.io.
The intended first public hard-cut release remains 0.24.0.

### ✨ Added

- Simplify the checked packed-word conversion so opt-z can eliminate a per-word
  panic-helper call. Keep one generic decode path and all preallocation bounds.

- Add private native codec hotspot benchmarks with selected ordinary/bulk
  workloads, separate timing/allocation builds, paired supplier samples and a
  symbol-bearing profiling loop. No benchmark dependency enters product crates.
  Extend investigation to i32/u64 varint boundaries and nested request shapes.

- Consolidate packed fields into `Packed<T, N>` with an optional element bound
  and one fallible constructor. Remove `PackedI16` and its special schema/runtime
  branch; every supported word kind now uses the same bounded representation.

- Prepare both product crates for registry publication with package guides,
  repository metadata, extracted-archive consumer checks and a Chromium smoke
  test of the shipped TypeScript runtime. The 0.23.8 coordinated publish dry run passed;
  publication remains explicitly deferred.

## [0.23.8] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Add scalar bounds and named synchronous browser validators through the shared
  schema graph. Keep one current, unversioned schema/runtime interface for the
  authorized hard cut; legacy compatibility is not a release requirement.

- Add explicit bool, finite-float, nonzero-i64 and heap-u64 bulk field adapters,
  sealed derive projections and matching browser validation. Qualify bytes,
  admission boundaries, allocation ordering and heap normalization against the
  reference; lint advice now names the available bool/float adapters.

- Add rooted bulk advice over the validated graph, with exact acknowledgements,
  stale-path detection, bounded recursive traversal and bounded diagnostic display.
  Payload/method client generation requires a diagnostic handler; the standalone
  consumer prints warnings and supports `--deny-warnings` before artifact output.

- Preserve standard Result, fixed-array, tuple, slice and Cow text schema layouts,
  including tuple structs and multi-field enum variants, across Rust and both
  TypeScript runtimes. Fixed arrays and tuples carry no sequence-length prefix.
- Expose published ByteBuf/ByteArray types and sealed standard serde_bytes field
  projection, retaining ordinary-array versus bulk-byte distinctions.
- Add optional host schemas for published Platform Types 0.20.4 Principal and
  Account, preserving absent versus explicit-zero subaccount bytes.
- Add finite float wrappers and browser projection, preserving finite IEEE-754
  bits and refusing nonfinite values and inexact browser f32 inputs.
- Add explicit packed integer and 16/32-byte identifier fields over the shared
  bounded-i16 bulk path, with owned decoding and partial-word rejection.

These are local evaluation additions, not a production migration or performance
acceptance. Browser/distribution and matched performance qualification remain;
see [the current parity ledger](docs/design/parity.md). The standalone Wasm size
gates passed at both optimization levels after sharing the packed implementation;
that measurement predates the subsequent lint, domain-adapter and validation work.

### 🐛 Fixed

- Keep qualification helper modules out of Cargo's standalone integration-test
  discovery; the three explicit test binaries retain their existing ownership.

### ▶️ Try it

Run `make check-packages`, then use the generated client directory it prints:
`make check-browser CLIENT_DIR=/absolute/path/to/run/client BROWSER=/absolute/path/to/chrome`.
These commands verify local archives and clients; they never publish. See
[release preparation](docs/release.md) for prerequisites and later delivery steps.

## [0.23.7] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ⚡ Improved

- Combine visit-budget checks and apply measured Wasm meter inlining, preserving
  admission behavior. The standalone consumer is smaller than the reference at
  both opt-z and opt-3; record the opt-z cost relative to the initial candidate.
- Append ordinary UTF-8 spans together in the bounded JSON/TypeScript writer,
  preserving escaping and exact output admission. The unchanged host probe shows
  lower export cost; single-root total-cost regressions remain unresolved.

### ✨ Added

- Add installed IC instruction qualification with a separately locked native
  simulator driver and optional guest counter. Record reproducible encoding
  regressions while preserving exact frame and independent size evidence.
- Add fixed-profile, fresh-build Wasm comparison with artifact execution, source
  hashes and total/code-section sizes. Retain the initial opt-3 regression and
  final passing results for the standalone consumer.
- Add an independent qualification consumer with a frozen sample-upload contract,
  candidate/reference supplier builds, generated TypeScript clients and runnable
  SDK-free Wasm artifacts. Qualify literal frames and typed refusals.
- Add a bounded paired host-schema timing probe for collection, finalization and
  JSON export. Record initial small-registry regressions without accepting a
  performance tradeoff or claiming replacement readiness.
- Qualify named collection with 1,024 shared scalar and recursive roots, including
  deterministic registration order, finite shared definitions and exact graph
  admission boundaries.

## [0.23.6] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Add 24 paired packed-i16/ordinary-list qualification cases through 16,384
  elements, preserving distinct wire representations and traversal allowances
  across Rust and generated TypeScript clients.
- Qualify generated byte-buffer clients through 1 MiB against Rust and both
  TypeScript runtimes, covering writer growth, length prefixes, byte limits,
  frame identity and owned-buffer custody.
- Add strict TypeScript qualification for the runtime and generated clients using
  a private pinned compiler. Check valid consumer calls and compile-time rejection
  of incorrect field types, decode results and request/response shapes.
- Add one candidate TypeScript interpreter for the admitted schema subset, exported
  as `typescript::RUNTIME`. Run generated payload/frame clients against candidate
  and published runtimes; compare primitive acceptance, typed failures and budgets.
- Add bounded named Rust schema collection for shared and recursive types.
  Retain reference-generated names, recheck encountered projections, and reject
  duplicate roots, name collisions, unstable projections and alias-only cycles.
  Generate the recursive framed-client fixtures from derives.
- Project explicit symmetric Serde field/variant renames into generated clients.
  Preserve Postcard bytes and reject duplicate projected names; qualify renamed
  client exchange and prototype-like field labels. Directional renames and other
  unsupported layout attributes remain explicit errors.

## [0.23.5] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Add optional strict metadata derivation for ordinary structs/enums and field
  adapter projections. Generate the representative payload client directly from
  its Rust type, admitting the result through the existing validated graph.
  Support concrete type/const generics and borrowed fields without a static
  lifetime requirement. Reject unsupported Serde layouts and recursive projection;
  preserve the explicit named-metadata path for recursive schemas.

## [0.23.4] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Generate typed request/response codecs with duplicate export/ID admission,
  exact u64 method identities and separate directional policy. Share module
  metadata and runtime admission with payload generation. Exercise emitted
  frames, asymmetric request/response schemas and the maximum method ID in Rust
  and the reference TypeScript runtime.

## [0.23.3] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Generate executable typed payload clients from the validated host graph, with
  private metadata, explicit browser limits, bounded total source and runtime
  format admission. Exercise the emitted client in representative Rust exchange,
  including policy observation and mismatch refusal before codec creation.

## [0.23.2] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Qualify wide integer and UTF-8 exchange with the published reference TypeScript
  runtime. Cover literal extremes, BigInt/range refusal, BOM preservation,
  malformed UTF-8, byte-counted text bounds and retained noncanonical integers.

## [0.23.1] - 2026-11-15

Versioned evaluation checkpoint; not published to crates.io.

### ✨ Added

- Qualify recursive records and shared named schemas through the candidate graph
  and published reference TypeScript runtime, with literal bytes, value custody
  and recursive depth-limit refusal.

## [0.23.0] - 2026-11-15

Initial evaluation checkpoint; not published to crates.io.

### ⚠️ Breaking

- Evaluate the next minor version after published ic-wire 0.22.4 with root-level
  codec APIs and explicit Encoding selection. This incomplete replacement remains
  unpublished; consumer migration and release qualification are still pending.

### 🛠️ Tooling

- Initialize the virtual Rust workspace with compiler 1.100.0 and shared lint
  policy. Install Toko Tooling 0.44.3 and expose its dependency list/update and
  tool setup commands. The real core package and lockfile enable Cargo admission.

### ✨ Added

- Export bounded reference-format JSON from the admitted host graph. Qualify
  representative payload/request/response byte exchange between candidate Rust
  and the published reference TypeScript runtime, including selected-span custody,
  truncation categories and byte/node/depth boundaries. Check request and response
  allowances independently without changing codec policy.
- Add optional host-only metadata admission into an immutable typed registry and
  bounded TypeScript declaration rendering. Validate references, alias cycles,
  names and constraints once; retain productive recursion and shared references.
  This initial subset does not yet generate a browser codec or collect Rust types.
- Start the portable core with method-ID derivation and fixed request-header
  validation, preserving reference identities and typed rejection behavior.
  Add literal contract vectors and native/Wasm development checks.
- Add bounded Postcard decoding through one payload/frame API, preserving
  borrowed input, reference traversal accounting and primitive acceptance.
  Check resource boundaries, hostile counts and typed failure precedence.
- Add shared encoding preflight for owned, reusable and slice output, with
  Postcard primitive delegation and failure-custody tests. Preserve frame bytes
  and reference traversal charges without private integer or enum-tag writers.
- Add bounded bytes, UTF-8 text, ordinary lists and input-backed borrowed views.
  Preserve field error semantics and test literal record encodings, ownership,
  borrow lifetimes and refusal before decoding excess list elements.
- Add explicit bounded packed i16 fields and a native representative record
  with ordinary-vector controls. Check little-endian bytes, element bounds,
  unaligned decoding, owned custody and bulk admission without unsafe code.
- Add a separate native compatibility workspace against published ic-wire
  0.22.4, with exhaustive short-primitive checks, representative budget sweeps,
  frame mutations and output-custody comparisons.
- Add isolated allocator qualification for preflight/output ordering, bounded
  field rejection and borrowed decoding, with successful-allocation controls and
  explicit coverage of incremental decoding and custom Serde allocations.
