# Changelog

## [0.7.19] - Unreleased

### 📝 Summary

- Add caller-storage and scoped payload encoding, plus TypeScript payload codecs
  and host generation matching Rust's existing unframed API. Preserve existing
  framed bytes, resource admission and error variants; no consumer migration is
  required for existing APIs.

### ✨ Added

- Encode into caller slices or select scratch/owned output once before encoding
  with `native::with_encoded`. Document complete-success publication, input drop,
  short-buffer errors and the unchanged two-pass admission contract.
- Generate and execute payload-only request/reply clients without a method ID,
  including zero-byte unit, standalone schema validation and named scalar checks.
  Document tested consumer scalar wrappers and JavaScript delivery in
  [SDK integration](docs/sdk-integration.md), addressing
  [#29 — Add payload-only client codecs and bounded caller storage for SDK integration](https://github.com/dragginzgame/ic-wire/issues/29).

## [0.7.18] - 2026-10-26

### 📝 Summary

- Adopt Shared Tooling 0.26.21, IC Build/Testkit and the release editor 0.68.8,
  and Platform 0.6.19. Keep the current root changelog's emoji section layout.
  Runtime/generator code, wire bytes and admission rules are unchanged.

## [0.7.17] - 2026-10-26

### 📝 Summary

- Adopt Shared Tooling 0.26.20 and align IC Build/Testkit and the release editor
  with 0.68.7, plus Platform 0.6.18. Use the common task-specific reading and
  validation rules without a local exception. Runtime, generator, wire bytes
  and admission rules are unchanged.

## [0.7.16] - 2026-10-26

- Record five further encoder experiments, including matched IC counters, uncompressed
  Wasm/code-section sizes and allocation controls. Reject inlining regressions
  and keep one-pass growth as research; production encoding and protocol remain
  unchanged.

- Decode u16/i16 varints with three explicit byte-group checks instead of a
  tail loop. Wide-list IC decode improves in both measured profiles; retain
  small controls, native timing, Wasm-size tradeoffs and rejected experiments.
  Payload bytes, shortest-form/overflow rejection, budgets and typed errors
  are unchanged (for example, `ff ff 03` remains u16 65535; `80 00` and
  `ff ff 04` remain malformed). No consumer migration is required.

## [0.7.15] - 2026-10-26

- Adopt Shared Tooling 0.26.18 at `37652c0`. Normal releases start from current
  committed metadata while explicit resume retains its recovery checks. Qualify
  Wire's adapter with malformed historical plans and retained evidence; update
  the expected early dirty-source diagnostic. Clarify that partial publication
  selection applies only to the same package version. Codec/protocol unchanged.

## [0.7.14] - 2026-10-26

- Allow explicit `PUBLISH_PACKAGES` selection when resuming a partial registry
  publication, keeping both public crates as the default and preserving the lint
  gate. Reject empty/private selections and qualify the recipes with a stub Cargo
  in disposable repositories. Runtime, generator and protocol are unchanged.

## [0.7.13] - 2026-10-26

- Reject duplicate numeric method IDs during host TypeScript generation, across
  names, query/update modes and equivalent spellings such as `7`/`07`. Previously
  same-ID u8/i8 methods could interpret payload `ff` as 255/-1. Conflicting
  registries must assign distinct IDs and coordinate clients. Valid generated
  source, runtime frame bytes and admission remain unchanged; consumers still
  own cross-registry schema binding and exact retained retry/signature bytes.

## [0.7.12] - 2026-10-25

- Document minimal unsigned big-endian magnitudes over existing `Bytes<N>` for
  lossless evidence beyond u128 (#26). Qualify external Nat conversion and exact
  Rust/browser option frames, including overflow values and admission failures.
  Zero is empty bytes; consumers reject leading zeroes at their numeric boundary.
  No new runtime type, codec, Candid dependency or protocol rule is introduced.

- Qualify default bulk advice through a real Cargo build script, including
  per-field suppression and stale acknowledgements, in the TypeScript check.
  Add a private IC admission-budget probe with valid/truncated controls. Reject
  multi-byte parser sharing after measuring Wasm growth and instruction
  regressions; keep the production codec and all protocol rules unchanged.

- Strengthen adversarial qualification for shared nested-list budgets and forged
  lengths with truncated prefixes. Record the Canic diagnostic-adoption gap and
  a rejected Wasm parser-sharing experiment with matched Candid/IC counters.
  No runtime, wire-byte, identity, admission-rule or generated-client change.

## [0.7.11] - 2026-10-25

- Run bulk field advice automatically during host TypeScript generation. Replace
  the silent three-argument `generate_registry` API with an acknowledgement map
  and mandatory diagnostic handler. Exact-path reasons silence intentional
  choices; stale acknowledgements remain visible. Consumers must connect the
  handler to their build output. Reuse the existing bounded lint traversal without
  adding a warning feature or runtime machinery. Generated source, protocol bytes,
  identities and runtime admission are unchanged.

- Refresh the full Wire/Candid comparison with released 0.7.10, the focused
  hotspot corpus and matched 0.7.9 regression checks. Retain IC instructions,
  frame bytes, native timing/allocation/heap samples and uncompressed Wasm/code
  sizes with reproducible source and analysis. Clarify that benchmark correctness
  checks do not run the performance matrix. Production code, protocol bytes,
  admission rules and generated clients are unchanged.

## [0.7.10] - 2026-10-25

- Parse derive field attributes once and reuse the validated projection for
  metadata labels and generic bounds. Serialize borrowed registries directly
  during TypeScript generation instead of building a duplicate JSON tree.
  Generated tokens/client source, protocol bytes and acceptance rules are
  unchanged; collision, cycle and traversal checks remain intact.

## [0.7.9] - 2026-10-25

- Add host-only `lint_bulk_roots` for field/use-site bulk review across interned
  registries. Explicit root labels and primitive-word path suffixes preserve
  independent acknowledgements, with bounded reference traversal and recursion
  guards. Existing definition-level lint semantics, wire bytes, admission and
  codecs remain unchanged. Addresses
  [#25](https://github.com/dragginzgame/ic-wire/issues/25).

## [0.7.8] - 2026-10-25

- Project optional `serde_bytes` adapter fields as options of byte buffers,
  including aliases and qualified generic fields. `None` / `Some(empty)` remain
  distinct (`00` / `01 00`); runtime bytes and admission are unchanged. Addresses
  [#23](https://github.com/dragginzgame/ic-wire/issues/23).
- Support symmetric Serde field/variant renames in native metadata; reject
  duplicate labels and retain refusal of asymmetric/custom attributes. Labels
  change browser keys/tags, not field order or variant ordinals. Regenerate
  clients while preserving existing consumer labels. Addresses
  [#24](https://github.com/dragginzgame/ic-wire/issues/24).

## [0.7.7] - 2026-10-25

- Grow TypeScript encode buffers incrementally instead of allocating the full
  directional byte allowance for every call. Preserve owned outputs, exact wire
  bytes, admission rules and custom-validator visits. Fixes the allocation issue
  in [#22](https://github.com/dragginzgame/ic-wire/issues/22); generated clients
  need regeneration to receive the change, with no contract migration.
- Add host-only `SchemaRegistry` collection for named recursive native metadata,
  including mutual recursion and concrete generic types. Add validated directional
  `method_limits` projection and cross-runtime admission boundary fixtures.
  Wire bytes, envelopes and runtime admission counters remain unchanged. Addresses
  [#20 — recursive registries](https://github.com/dragginzgame/ic-wire/issues/20) and
  [#21 — browser budget projection](https://github.com/dragginzgame/ic-wire/issues/21).
- Simplify the private Postcard reader to its sole byte-slice input and remove an
  unused finite-float macro argument. Public APIs, wire bytes and admission rules
  are unchanged.
- Centralize benchmark cohort selection and share one source snapshot across
  matrix runs while keeping standalone runs self-contained. Separate historical
  experiments from the current evaluation guide without deleting evidence.

## [0.7.6] - 2026-10-24

- Strengthen adversarial decode qualification: verify envelope rejection before
  consumer deserialization, bounded element visits for forged zero-byte collection
  lengths, and independent calls after failures. Clarify consumer integration
  responsibilities; runtime code, wire bytes and acceptance rules are unchanged.

## [0.7.5] - 2026-10-24

- Support host-only `Cow<str>` text metadata, including transparent consumer
  identifiers. Qualify owned/borrowed Rust and TypeScript frames, admission and
  native/Wasm feature forwarding; runtime bytes and codec behavior are unchanged.
  Resolves [#19 — Support Cow<str> native schema projection for transparent consumer identifiers](https://github.com/dragginzgame/ic-wire/issues/19).
- Confirm the delivered native metadata and schema-forwarding fixes against the
  successful 0.7.3 Linux/macOS matrix and close
  [#17 — Native Serde schema projection for endpoint-derived TypeScript registries](https://github.com/dragginzgame/ic-wire/issues/17).

## [0.7.4] - 2026-10-24

- Decode one-byte u16/u32/u64 varints directly while retaining width-specific
  multi-byte validation. Matched IC tests show up to 11% lower decode cost with
  nearly neutral Wasm size; retain the small-input regressions in the evidence.
  Encoding, wire bytes, limits and malformed-input acceptance are unchanged.

- Record two rejected shared-varint reader prototypes: Wasm savings did not
  justify wide/small-integer decode regressions. Retain correctness and matched
  instruction evidence; production decoding and protocol remain unchanged.

- Reject byte-output inlining after broader tests exposed roughly doubled
  instructions for wide-u16 list encoding despite smaller type-count controls.
  Retain reproducible size and instruction evidence; production is unchanged.

- Record the rejected shared output-preparation experiment: negligible opt-z
  savings at 100 types were outweighed by growth in other builds. Retain
  reproducible size evidence and refresh the handoff; production is unchanged.

## [0.7.3] - 2026-10-24

- Reject the encoding-wrapper inlining candidate after five-cohort, two-profile
  qualification exposed encode regressions up to approximately 2x. Retain size,
  instruction and payload-parity evidence; production behavior remains unchanged.

- Retain measured Wasm-size research for scalar sharing, encoding-wrapper
  inlining and compiler optimization levels. Record workload-dependent size
  and instruction tradeoffs; leave production code and build profiles unchanged.

- Record the rejected Wasm budget-sharing experiment: roughly 1% size savings
  at 100 types did not justify higher instruction costs. Keep production
  inlining, runtime behavior and wire bytes unchanged.

- Reject incomplete benchmark measurements, nonpositive timings/sizes and
  mismatched input identities before writing comparisons, instead of accepting
  absent hashes, emitting invalid ratios or panicking on malformed fields.

- Require the packed-i16 benchmark lane when reporting that cohort, so native
  control measurements cannot hide missing packed results. Preserve both lanes
  in the comparison output; runtime and protocol are unchanged.

- Adopt IC Tooling 0.68.1 and IC Platform 0.6.15 for diagnostic builds,
  benchmark support and the release metadata editor. Keep published runtime
  dependencies and existing codec behavior unchanged.

- Fix schema-forwarding qualification on macOS Bash 3.2 by keeping Cargo
  argument arrays nonempty under `set -u`. Runtime and protocol are unchanged.

## [0.7.2] - 2026-10-24

- Adopt IC Tooling 0.68.0 for diagnostic builds, benchmark lifecycle helpers
  and the release metadata editor. Published runtime dependencies, wire bytes
  and codec behavior are unchanged.

## [0.7.1] - 2026-10-24

- Add opt-in host schema advice for ordinary primitive lists, exact-path
  acknowledgements with reasons, stale-acknowledgement detection and a
  `bulk-lint --deny-warnings` command for consumer CI. Existing schemas and
  codec behavior are unchanged; field adoption remains explicit.
- Support `Packed<i8>` with `Int8Array` clients. Signed-byte payloads retain
  ordinary vector bytes (`[-128, 127]` remains `02 80 7f`) but use bulk
  admission. Regenerate clients and review limits when adopting the field.
- Fix the benchmark gate's expected bulk corpus count to include the ten new
  signed-byte cases (109 total).

## [0.7.0] - 2026-10-24

- **Breaking:** Decode in one pass without reserializing the result. Decode APIs
  require only `Deserialize`; maps, sets and custom Serde visitors may normalize
  input. Consumers own ordering/uniqueness rules and must retain original frame
  bytes for exact retries, signatures and hashes instead of reconstructing them.
  For example, map bytes `02 02 14 01 0a` now decode successfully and re-encode
  as `02 01 0a 02 14`. Encoder bytes and framing are unchanged.
- Reject nonminimal varints and invalid char payloads directly as `Malformed`;
  remove the obsolete `Noncanonical` error. Decode applies its own resource
  allowance without a second serialization charge. Update exhaustive error
  matches and configure encode/decode budgets independently.
- Keep one Serde/Postcard codec: use a private attributed Postcard reader with
  strict primitive checks, retaining upstream encoding and no forked dependency.
  Remove the comparison implementation and qualify normalization, malformed
  inputs, borrowing and Deserialize-only consumers. No compatibility lane remains.

## [0.6.13] - 2026-10-24

- Add five mixed blob/packed-integer/string/optional records to hotspot
  qualification (87 cases). Isolate canonical comparison costs and reject two
  exact comparison alternatives after matched instruction and Wasm measurements.
  No runtime, wire-byte, admission or canonical-acceptance change is retained.

- Expand hotspot qualification from 50 to 82 cases with byte-buffer block
  boundaries. Record and reject larger canonical-comparison blocks: modest
  large-buffer instruction savings cost extra Wasm and regress small inputs.
  Production codec, protocol bytes, admission and canonical checks are unchanged.

- Expose `ic_wire::ByteBuf` as the standard `serde_bytes::ByteBuf` type, with
  alloc-only runtime support and native TypeScript byte metadata. Consumers need
  only ic-wire and Serde for owned bulk byte fields. Qualify the public path in
  no_std native/Wasm consumer builds, exact frames and TypeScript conformance.
  Existing byte-buffer encoding/admission and canonical checks are unchanged;
  ordinary `Vec<u8>` still uses per-element traversal unless explicitly adapted.

- Make byte-oriented binary fields and packed integer/hash arrays the default
  guidance for new bulk contracts. Add a compiled mixed-DTO quickstart and
  Rust/TypeScript conformance for empty, singleton and multi-element values;
  retain ordinary collections for general shapes and compact small values.
  Document coordinated field adoption, client types and preserved bounds.
  This guidance/qualification batch changes no production encoding or admission;
  existing fields are not automatically rewritten.

- Reduce ordinary i16 encoding costs with a direct, byte-identical Postcard
  varint writer. Matched standard-corpus wide-list IC encoding falls 54% at
  opt-z and 34% at opt-3; the standard Wasm grows 370/673 bytes. Record the
  opt-z small-u16 instruction and native wide-i16 decoding regressions, rejected
  alternatives and repeated measurements. Add a focused hotspot corpus.
  Protocol, schemas, admission, canonical hashing and retry bytes are unchanged
  (`32767_i16` remains `fe ff 03`); no consumer migration is required.

- Expand and rerun the Wire/Candid matrix: 21 cohorts, 1,101 overlapping case
  entries, both profiles, IC instructions, frame bytes, native timing/memory and
  84 uncompressed Wasm artifacts. Add integer/container/generic/mixed-bulk cases
  and a reproducible full runner. Preserve all wins, losses and matched controls;
  production codec, wire bytes, acceptance rules and hashing are unchanged.

- Keep TypeScript metadata and its optional dependencies native-only even when
  a consumer forwards the schema feature into a Wasm build. Derives still leave
  Serde implementations and runtime codecs available. Add consumer/graph checks
  alongside existing strict TypeScript conformance. Fixes the remaining feature
  boundary in [#17](https://github.com/dragginzgame/ic-wire/issues/17); wire bytes,
  acceptance rules, canonical hashing and limits are unchanged.
- Adopt Shared Tooling commit `4c0f198e` (recorded VERSION 0.26.10), including
  Cargo setup phase/timing diagnostics, literal tool catalogs and aggregate tool
  diagnostics. Keep the canonical snapshot and common external IC guard. The
  supplier's new changes are headed 0.26.11, while its committed VERSION remains
  0.26.10; retain exact revision-based provenance.
- Adopt IC Tooling 0.67.0 for Build/Host/Testkit qualification and the release
  metadata editor. Preserve native-only dependency roles and existing feature
  selections. Codec bytes, admission, hashing and production runtime are unchanged.


## [0.6.12] - 2026-10-24

- Restore deterministic packed fuzz seeds on every run, without requiring the
  archived corpus. Add Rust endian-helper tests and simulated big-endian
  TypeScript codec checks to normal qualification. Real big-endian execution
  remains unqualified; production code paths and protocol are unchanged.

- Strengthen packed-field qualification across length-prefix boundaries, input
  offsets, malformed bytes and exact resource limits; add all seven packed types
  to the runtime fuzz model. Record native code-layout and TypeScript small-array
  experiments, rejecting candidates without a reliable overall win. Production
  codec, bytes, acceptance rules and generated runtime remain unchanged.

## [0.6.11] - 2026-10-24

- Qualify the Blob-shaped manifest boundary for packed hashes, preserving exact
  hash/header bytes and frame-limit rejection without changing consumer storage.
  Replace the long current-comparison page with an index, preserve historical
  studies and repair their deep links. Reject internal packed sharing candidates
  after Wasm screening; production runtime, bytes and acceptance are unchanged.

- Promote the measured bulk field to `Packed<T>` for six integer widths and
  32-byte hashes, with Serde and TypeScript metadata/typed arrays. Keep one
  Postcard codec and all canonical/admission checks. Opt-in fields replace an
  element count/varints with byte count/fixed little-endian words: `[32767_i16]`
  changes from `01 fe ff 03` to `02 ff 7f`. Consumers must coordinate schemas
  and retain retry bytes; small integers can grow. Existing `PackedI16<N>` keeps
  its field bound and bytes. Retire the duplicated benchmark implementation.
  Qualify the public implementation against the prototype and Candid, retaining
  IC/Wasm parity and documenting native/TypeScript timing tradeoffs.

- Record isolated codec-cost ablations against Candid and upstream Postcard,
  separating budget, preflight, canonical-verification and output-writer costs.
  Retain measurements and rejected diagnostic sources only; production codec,
  limits and protocol are unchanged.

- Record the full thirteen-cohort Wire/Candid comparison: repeated native time,
  IC instructions, payload sizes, allocations, peak heap and uncompressed
  total/code-section Wasm in both profiles. Preserve all wins and losses;
  this measurement batch changes no runtime behavior or protocol.

- Share packed-i16 decoding across bounds while checking each field's exact
  element limit before allocation. Preserve wire bytes, resource admission,
  malformed-input errors and canonical retry behavior. Record the measured
  multi-bound Wasm savings and single-bound/decode-overhead tradeoffs.

## [0.6.10] - 2026-10-23

- Allow registry declarations and method exports named `types`, `createCodec`
  and `JSON`. Generated helpers use names outside the consumer identifier grammar;
  registry literals preserve own `__proto__` properties without global JSON access.
  Protocol bytes, payload acceptance and canonical retry rules are unchanged.
- Add packed-field scaling cohorts for 1/10/100 derived records with shared or
  varied bounds. Retain measured Wasm/IC evidence and reject a larger owned-blob
  comparison loop because its modest large-input gain adds Wasm and slows small inputs.

## [0.6.9] - 2026-10-23

- Fully qualify generated `Int16Array`, `Uint8Array` and `ReadonlyArray` types
  through `globalThis`, allowing consumer declarations with those names without
  shadowing built-ins. Payload schemas and wire bytes are unchanged.
- Include the packed-i16 fixture hash in native and IC benchmark identities,
  and refresh the released Serde/0.6.8 status documentation.

## [0.6.8] - 2026-10-23

- Add bounded `PackedI16<N>` Serde fields and matching `Int16Array` TypeScript
  metadata/codecs, using safe bulk byte views for little-endian encoding and
  owned decoding with bounds checked before allocation. Ordinary `Vec<i16>` is
  unchanged. Opting in changes `[32767]` from `01 fe ff 03` to `02 ff 7f`;
  coordinate consumers, regenerate clients and resolve retained old-layout frames.
  Small values use more bytes; framing, identity and canonical retry rules remain.
  Replace benchmark-only adapters/conversion paths with the production field;
  retain their archived research and measure the production cohort against Candid.

- Adopt IC Build/Testkit and Host support crates 0.66.0, including the matching
  release metadata helper. Update the instruction benchmark to borrow its
  PocketIC client through the owner accessor, keeping the managed server alive
  throughout use. Production codec, protocol bytes and admission are unchanged.

## [0.6.7] - 2026-10-23

- Expand the standard Candid comparison from 154 to 214 cases, adding signed and
  mixed-width integer distributions, byte boundaries, float/boolean/optional
  lists, nested blobs and account snapshots. Rerun native time, allocation,
  peak heap, IC instructions, wire bytes and uncompressed Wasm across standard,
  Toko-shaped and 1/10/100-record cohorts. Stop generating gzip measurements.
  Benchmark-only changes; production runtime, protocol and acceptance are unchanged.

- Share i64 zigzag encoding with the existing unsigned Postcard writer. Large
  small-i64 vectors use 48.78% fewer opt-3 encode instructions and 16.51% fewer
  decode instructions; the standard fixture shrinks by 988 uncompressed Wasm
  bytes; the 100-record fixture shrinks by 3,755 bytes. Small-u64 opt-z encoding
  regresses up to 12.79%; no standard opt-3
  regression is measured. Preserve signed-type admission, exact wire bytes,
  canonical checks and retry hashes. No consumer migration or second codec.

## [0.6.6] - 2026-10-23

- Extend the private one-byte Postcard shortcut to small u16/u32 values. Large
  small-value vectors use 61–63% fewer opt-3 encode instructions; the expanded
  fixture adds 601 uncompressed Wasm bytes. Wide-u16 opt-z encoding regresses
  up to 3.89%. Original budgets, wire bytes, canonical checks and retries remain
  unchanged; no consumer migration or additional codec. Extend integer controls
  and retain measured rejected signed, streamed-u64 and blob-comparison trials.

- Adopt Shared Tooling 0.26.7 and IC Build/Testkit with Host support crates 0.65.0.
  Explicit tool setup now waits for an admitted shared-slot installer; offline
  checks remain immediate. Keep host tooling out of the published runtime.

## [0.6.5] - 2026-10-23

- Forward small u64 values through Postcard's equivalent one-byte serializer.
  Large small-u64 lists use 40.6% fewer opt-3 encode instructions and 15.8% fewer
  decode instructions, adding 146 Wasm bytes in both standard and 100-type probes.
  Opt-z wide-list encoding regresses 1.70%. Preserve original u64 admission,
  wire bytes and canonical checks; no new codec, API or consumer representation.

- Record five rejected Serde short-write/inlining experiments with matched IC
  instructions, uncompressed Wasm sizes and full source/sample evidence. Large
  small-integer-list gains did not justify other regressions and opt-3 code growth.
  No runtime, protocol or dependency-selection change is retained.

## [0.6.4] - 2026-10-23

- Retire the obsolete finite-codec protocol benchmark runner and its native-view
  helper. Historical studies now link to the exact 0.5.22 source revision;
  retained measurements remain intact. Compact the accumulated handoff.
  Production Serde benchmarks, runtime APIs, bytes and admission are unchanged.

## [0.6.3] - 2026-10-22

- Support container `serde(deny_unknown_fields)` and qualified field-level
  `serde(with = "serde_bytes")` in metadata derives. Existing DTOs retain their
  serializers and bytes; byte fields generate Uint8Array bindings. Unsupported
  adapter/type combinations remain rejected. No principal support is added.

- Derive metadata bounds from serialized field types instead of requiring every
  generic parameter to implement `Wire`. Associated-type providers no longer
  need unrelated metadata implementations. Preserve recursive-type refusal and
  Serde's Serialize requirements; metadata layouts and runtime bytes are unchanged.

## [0.6.2] - 2026-10-22

- Describe Serde slices and boxed slices in `Wire` metadata, sharing the list
  projection with Vec. Previously rejected slice fields now generate matching
  TypeScript arrays; existing Vec metadata/budgets and protocol bytes are unchanged.

- Support Serde's variant-level `borrow` annotation on newtype enum variants
  in `Wire` metadata. Bare and explicit lifetimes preserve the existing variant
  tag and borrowed payload; invalid unit/tuple/record placements remain rejected.
- Reuse the scalar metadata helper for finite floats, removing duplicate
  implementations without changing metadata, generated clients or runtime code.

## [0.6.1] - 2026-10-22

- Allow field-level `serde(borrow)` and explicit borrowed lifetimes in `Wire`
  metadata derives. Borrowed bounded DTOs can now derive metadata alongside
  Serialize/Deserialize, matching their owned forms. Custom-layout attributes
  remain rejected; runtime admission, frame bytes and client layouts are unchanged.

## [0.6.0] - 2026-10-22

- Gate the byte-buffer metadata doctest on the required `typescript` feature
  and document its setup, so default-feature documentation tests also pass.

- Support `serde_bytes::ByteBuf` and borrowed `serde_bytes::Bytes` in host-only
  `Wire` metadata, generating `Uint8Array` for the existing bulk byte codec path.
  Replacing `Vec<u8>` preserves Postcard bytes but changes traversal accounting
  and generated client types; regenerate bindings and qualify consumer limits.
  No runtime codec change or automatic Serde derive rewriting.

- Hard cut to one production Serde/Postcard codec. Remove the finite `Wire`
  derive/trait, static shape admission, typed method/retained-byte API and finite
  TypeScript generator. Keep bounded field wrappers as ordinary Serde types.
  Use `native` framing with explicit IDs/budgets and host-only `Wire`.
  Former ID-sorted fields now follow Serde declaration order; enum tags are
  zero-based indices. Coordinate affected consumers and regenerate metadata;
  existing native frames are unchanged. No legacy reader or compatibility shim.
  Migrate benchmarks, borrowing, allocation and cross-language checks to the
  shipped codec; previous finite-codec savings are research evidence only.

- Record the full five-cohort Serde-derive prototype comparison against Candid:
  IC instructions, native timing, allocation/heap costs, frame bytes and
  uncompressed Wasm/code-section sizes. Include Candid wins and retain raw
  reproduction evidence. This experiment changes no production codec or protocol.

- Add host-only `Wire` metadata derivation for ordinary native Serde
  declarations, including concrete generics, records, enums and transparent
  wrappers, without field IDs or replacement serializers. Reuse the existing
  TypeScript registry/runtime; reject unsupported attributes and recursive inline
  schemas. Custom scalar projections require explicit metadata and verified
  validators. Existing wire bytes, codec costs and admission rules are unchanged;
  native declaration order remains part of the consumer's protocol.
  Name the metadata-only derive `ic_wire::Wire`, replacing the unreleased
  `NativeSchema` name without an alias. It requires separate Serde derives and
  does not restore the former finite serializer or change runtime bytes/costs.

- Adopt IC Tooling 0.64.1 for Build, Testkit and transitive Host libraries,
  aligning the release metadata editor with the library selection. Existing
  codec APIs, production dependencies and wire bytes are unchanged.

## [0.5.22] - 2026-10-22

- Avoid rescanning directly decoded bounded bytes during finite canonical
  verification. Generated records delegate verification only when every field
  preserves roundtrip boundaries; custom children retain whole-value comparison.
  A 1 MiB owned blob uses 58.4% fewer opt-3 IC decode instructions, and the
  100-type fixture's code section shrinks by 8,403 bytes. A one-byte blob costs
  17 extra instructions. Ordinary Serde decoding and protocol bytes are unchanged.

- Share fixed-header validation on Wasm for a small code-size saving, trading
  additional decode instructions. Strip symbols in benchmark profiles for both
  codecs and document the consumer setting; smaller files lose diagnostic names.
  Most file savings are metadata removal, not reduced executable code.

- Share finite output allocation/header setup on Wasm, keeping one codec strategy.
  The 100-type opt-3 code section shrinks 2.49%, trading 0.49–0.68% more encode
  and 4.23–5.66% more decode instructions.
  Protocol bytes, allocation policy and public APIs are unchanged.

- Add isolated Toko-shaped and 1/10/100-distinct-type derive benchmark cohorts,
  sharing the native, allocation, heap, IC instruction and Wasm-size runner.
  Preserve ULID blob representation and label the token-card projection and
  preformatted decimal limitation. Report code-section size separately from total
  Wasm and gzip, and exclude per-type debug formatting from Wasm assertions.
  Runtime dependencies and protocol are unchanged.

- Inline native serialization admission and compound forwarding on Wasm. The
  comparison fixture shrinks 5,449/1,569 Wasm bytes at opt-z/opt-3; opt-3
  1,024-hash manifest encoding/decoding uses 20.0%/18.8% fewer IC instructions.
  Some small opt-z cases regress up to 2.08%. Protocol bytes, limits, typed
  errors and native-target behavior are unchanged.

## [0.5.21] - 2026-10-22

- Adopt Shared Tooling 0.26.6 and IC Tooling 0.64.0 for Build, Testkit and
  transitive Host libraries; prepare the 0.64.0 release editor with `cli`.
  Runtime dependencies, protocol bytes and consumer APIs are unchanged.

- Store native allocation allowances in whole charge units, preserving limits,
  overflow refusal and protocol bytes. The comparison fixture shrinks 326/796
  Wasm bytes at opt-z/opt-3; text-pair encoding uses 3.04% fewer IC instructions
  at opt-3. Two malformed-map cases regress less than 0.025%. Document
  byte-oriented Serde blob encoding and assert matching generic/blob frames.

- Reduce owned byte/text decoding instructions on Wasm with a separate bulk
  comparison loop. A 1 MiB blob improves 8.7% at opt-z and 6.1% at opt-3 for
  200/327 additional linked Wasm bytes; some small inputs cost up to 73 extra
  instructions. Protocol bytes, admission and typed failures are unchanged.

## [0.5.20] - 2026-10-22

- Resolve benchmark Wasm artifacts from Cargo build messages so configured target
  directories cannot select stale files. Reject incomplete or duplicate comparison
  rows before writing reports; Wire-only reuse measurements remain explicit.

- Reduce Wasm canonical byte/text comparison instructions with 32-byte bulk
  blocks and word-sized tails. Measured 1 MiB owned decode improves 14.0% at
  opt-z (+129 linked Wasm bytes) and 2.38% at opt-3 (+25 bytes); protocol bytes,
  admission, typed failures and native comparison behavior are unchanged.

- Add an unpublished Candid comparison suite with matched wire types, native
  timing, allocation counts, peak live heap, IC instructions through PocketIC,
  and linked/compressed Wasm sizes.
  Candid remains a benchmark-only dev dependency through IC Platform; the
  production protocol and published crates are unchanged.

- Prevent generated Wire generic/visitor names from shadowing contract payload
  types, fixing compilation for names such as `A` and `D` without changing bytes.
- Fully qualify generated core/Serde references, including result constructors
  and primitive enum tags, to avoid consumer-name and prelude dependencies.
- Use private generated bindings to avoid ordinary consumer constants such as
  `value` and `tag`; infer enum payloads so associated types using `Self` compile.
- Remove an implied finite-frame length check and share TypeScript generation
  safe-integer limits, preserving protocol bytes, admission and typed errors.

## [0.5.19] - 2026-10-21

- Rename the unpublished tooling crate to `ic-wire-qualification` and its check
  target to `check-qualification`; retain allocation/borrowing and diagnostic checks.
- Retire alternative-codec comparison suites, their benchmark targets and
  bitcode/wincode/MessagePack dependencies. Preserve historical reports and evidence.

## [0.5.18] - 2026-10-21

- Centralize native decoder admission, byte/text budget checks and compound encoder
  forwarding, reducing duplicated checks while preserving protocol and error behavior.

## [0.5.17] - 2026-10-21

- Qualify native borrowed bytes/text with decode-plus-scan measurements and
  consumer applicability notes; preserve the existing protocol and runtime.

- Measure exact/headroom bulk reservations, including threshold and multi-chunk
  cases; retain current growth after observing capacity and timing tradeoffs.

- Evaluate and reject native word-reduction and early-exit canonical comparisons
  after repeated owned-blob regressions; preserve the released comparator.
- Allow isolated benchmark candidates from external files and retain their patches,
  paired timings and unchanged-frame checks.

## [0.5.16] - 2026-10-21

- Skip redundant canonical byte comparisons for borrowed data on native hosts,
  matching the existing Wasm shortcut without changing bytes or rejection rules.
- Measure consumer layout tradeoffs and canonical-decoding stages; keep schema
  cuts experimental and retain full canonical verification.

- Add a reproducible native-registry generation benchmark and measured evidence;
  require benchmark reports and inline protocol-change explanations for future work.

- Check finite enum variant counts against the generation budget before copying
  and sorting them, avoiding unnecessary allocation for oversized metadata.
- Consolidate finite-schema name and enum-tag validation in the shared metadata
  validator; check record ID ordering with adjacent comparisons.
- Replace repeated prefix scans with ordered sets when checking duplicate native
  metadata fields, enum tags and method names, preserving generated source order.

## [0.5.15] - 2026-10-21

- Reject non-string scalar-check annotations during TypeScript generation,
  including annotations on named references and nested values.
- Reject `as` as a TypeScript registry declaration name during generation,
  returning a schema error instead of emitting source that fails to compile.

## [0.5.14] - 2026-10-21

- Fix TypeScript byte decoding from Node Buffer inputs to return owned storage,
  preserving isolation when either the input frame or decoded bytes are mutated.
- Expand TypeScript length-prefix checks across the full one-byte domain and
  malformed u64 prefixes, preserving typed limit and malformed-input errors.
- Evaluate and reject a short-length decoding fast path after repeated
  longer-text regressions; retain the existing runtime implementation.

## [0.5.13] - 2026-10-21

- Reuse number-based varints for TypeScript enum tags, preserving canonical u32
  IDs and malformed-input refusal without changing the wire format or public API.
- Qualify enum tag width boundaries and maximum IDs; record native Node timings.

## [0.5.12] - 2026-10-21

- Encode and decode TypeScript 8/16/32-bit integers with number arithmetic,
  preserving canonical Postcard bytes and bigint handling for wider integers.
- Evaluate and reject two text-buffer optimizations after measured regressions;
  preserve the existing text encoder and expand its boundary checks.
- Expand integer-boundary, nested-limit and owned/borrowed malformed-input checks.

## [0.5.11] - 2026-10-21

- Adopt Shared Tooling 0.25.9: refuse a new release increment past an
  undelivered committed preparation, preserving its saved plan and evidence.
  Tool pins, package APIs and dependency selections are unchanged.

## [0.5.10] - 2026-10-21

- Reserve finite output ceilings up to 128 bytes before writing the header,
  avoiding a second reservation for small control messages crossing 64 bytes.
  Tiny values may retain up to 64 additional requested bytes. Fixes #14.

## [0.5.9] - 2026-10-21

- Record the TypeScript bounded-growth writer experiment: large frame allowances
  benefit, but tight blob/array regressions leave the existing writer unchanged.

## [0.5.8] - 2026-10-21

- Enforce declared TypeScript float bounds when encoding and decoding, and
  reject malformed or inverted numeric bounds during registry generation.
  Integer bounds outside JavaScript’s exact range are refused before rounding.
- Encode and decode TypeScript floats through shared frame views, avoiding
  temporary byte arrays and views per value while preserving exact frames,
  buffer offsets and limits.
- Correct the packaged dependency example to select the current 0.5 API.

## [0.5.7] - 2026-10-21

- Adopt IC Tooling 0.63.0 for diagnostic builds and the release metadata editor.
  Select the editor's required `cli` feature separately from library consumers.

- Avoid repeated TypeScript registry enumeration within each codec operation,
  preserving alias checks and registry changes between calls.
- Reject invalid optional collection bounds and inverted byte-length bounds
  during TypeScript generation instead of relying on JavaScript coercion.

## [0.5.6] - 2026-10-21

- Reject TypeScript registry names that shadow generated JSON handling,
  transparent alias cycles, invalid fixed-array lengths and method budgets
  that cannot initialize a codec.
- Include TypeScript conformance, generator tests, linting and minimum-Rust
  checks in CI and release validation, with pinned Node/npm setup in hosted CI.

## [0.5.5] - 2026-10-21

- Separate codec-only and application-checksum allocation benchmarks with
  `--codec-only`, independent encode/decode calibration and labelled format-2
  reports. Retain measured finite-buffer reuse and pre-allocation UTF-8
  experiments; text decoding remains unchanged.
- Add finite `encode_request_into` and `encode_response_into` for reusable
  scratch buffers, preserving admission, canonical frames and bounded output.
  Retained retry bytes remain independent of mutable scratch storage.

## [0.5.4] - 2026-10-20

- Update the diagnostic tooling dependency to IC Build 0.62.0 and its transitive
  Host Artifacts, FS and Process dependencies to 0.62.0. The separately pinned
  release metadata editor remains at 0.24.0.

## [0.5.3] - 2026-10-20

- Adopt IC Build 0.24.0 for diagnostic tooling and the pinned release metadata
  editor, aligning its transitive Host dependencies with 0.32. Runtime codec
  dependencies and wire behavior remain unchanged.

## [0.5.2] - 2026-10-20

- Retain the [native pointer-comparison experiment](docs/evaluation.md#native-borrowed-pointer-equality-experiment)
  as rejected evidence: borrowed blob gains came with packed-manifest regressions.
  Keep the existing runtime behavior.

- Adopt Shared Tooling 0.25.8 at `0a2d2ee`, including numbered Unreleased
  changelog handling and retained metadata failure evidence. Select the pinned
  IC Build release editor automatically, run Clippy before expensive release
  checks or package effects, and use Clap for evaluation and conformance command-line parsing.

- Initialize the finite encoder header directly in its bounded starting buffer.
  Preserve output bytes, capacity and fallible allocation while avoiding the
  general growth path on every encode. Tiny Relay probe messages use 5.8–11.5%
  fewer encode instructions under opt-3; both measured profiles shrink linked
  Wasm. See [matched evidence](docs/evaluation.md#direct-header-initialization).

## [0.5.1] - 2026-10-20

- Evaluate and reject per-type codec preferences: mixed opt-3 saves 1.8% linked
  and 4.2% compressed Wasm, but opt-z linked size grows 3.9%. The maintenance
  cost does not justify adoption; no preference API is shipped. Retain the
  [experimental measurements](docs/evaluation.md#public-api-code-generation-adapter).

- Grow finite encoder output fallibly within the admitted schema ceiling instead
  of reserving the maximum for every tiny message. Preserve one-pass encoding,
  canonical bytes, typed failures and conservative admission. Relay-shaped small
  batches use 88–96% fewer encode instructions in isolated IC probes; document
  large-message, native-time and linked-size tradeoffs in the
  [evaluation evidence](docs/evaluation.md#bounded-finite-output-growth).

## [0.5.0] - 2026-10-19

- **Breaking Rust API:** `Wire` no longer implies owned deserialization. Generic
  decode callers must supply the appropriate Serde decode bound; encoding-only
  contracts are supported. Existing Postcard bytes, framing and identities remain
  unchanged.
- Add bounded `BytesRef`/`TextRef` views, owned-to-borrowed projections and
  lifetime-aware record/enum derives. Typed request/reply decoding can borrow
  input while preserving field bounds, canonical checks and TypeScript schemas.
  See the [migration and usage guide](crates/ic-wire/README.md#borrowed-bounded-contracts-and-the-05-api-cut).

- Qualify final borrowed contracts with native timing, IC instruction and linked
  Wasm probes; retain identical frame hashes and document full-scan tradeoffs in
  the [evaluation evidence](docs/evaluation.md#final-05-borrowed-contract-measurements).

## [0.4.10] - 2026-10-19

- Adopt IC Build 0.22 and its IC Host 0.30 dependency family for compiler
  diagnostics. Remove unused host dependency trees while preserving runtime
  dependencies, codec APIs and wire bytes.

## [0.4.9] - 2026-10-19

- Skip redundant Wasm canonical byte comparisons when a borrowed value emits
  the exact input slice, preserving limits and rejection behavior. Large borrowed
  decode-plus-scan probes use 12–17% fewer IC instructions; see the
  [qualification and hard-cut assessment](docs/bounded-allocation-study.md#follow-up-where-a-hard-cut-would-help).

## [0.4.8] - 2026-10-19

- Grow bounded lists geometrically within their declared maximum when decode
  length hints are hidden, avoiding repeated one-element reallocations.
- Investigate exact-size encoding and borrowed finite contracts; retain current
  APIs and allocation defaults based on the [measured tradeoffs](docs/bounded-allocation-study.md).

## [0.4.7] - 2026-10-19

- Allow native decoding to borrow input bytes/strings and add reusable-output
  encoding APIs, preserving canonical validation, admission and existing frames.
- Qualify bounded contracts, packed consumer layouts, borrowing and buffer reuse
  with native/IC measurements; see the [tradeoffs](docs/allocation-comparison.md).

## [0.4.6] - 2026-10-19

- Reduce Wasm budget-check and bulk canonical-comparison costs without changing
  bytes or resource admission. Blob-shaped probes show 30–39% fewer manifest
  encode instructions across speed/size profiles; see the
  [optimization report](docs/runtime-optimization.md) for decode gains and tradeoffs.

- Add a reproducible, unpublished MessagePack/Postcard benchmark for Blob-shaped
  workloads, with native timings, external IC instruction evidence and independent
  Wasm sizes. Keep Postcard as the production encoding; see the
  [comparison report](docs/messagepack-comparison.md).

- Reduce Wasm canonical bulk-byte comparison instructions using exact wordwise
  XOR/OR accumulation. Preserve bytes, resource admission and typed failures.
  Earlier consumer-owned Blob measurements show a 31% decode-instruction reduction for
  a 1 MiB opaque-byte control; manifest workloads remain essentially unchanged.

## [0.4.5] - 2026-10-19

- Qualify 1 MiB opaque buffers across finite Rust, native Rust and generated
  TypeScript, including exact frames and bounded malformed-input refusal. Blob
  service contracts remain consumer-owned; no runtime API or encoding changes.

- Remove endpoint SDKs, simulators, browser transport and the private adapter
  harness from every dependency role. Retain codec diagnostics, standalone
  Rust/TypeScript conformance and native raw-codec controls. Endpoint qualification
  belongs to consumers; the published codec API and wire bytes are unchanged.
- Retire format-comparison reports and obsolete integration instructions.

- Remove Cargo's duplicate license metadata warnings while retaining MIT metadata
  and identical regular LICENSE files in both publishable packages, compatible
  with the formatting hook. Preserve file types in its disposable test fixture.
- Select authored crate files automatically in the local Make fixture, including
  untracked modules and excluding ignored build output. New source modules no
  longer require a second hand-maintained file list for formatting qualification.
  Shared Tooling and the published library contract are unchanged.

## [0.4.4] - 2026-10-18

- Reduce canonical-comparison work on Wasm and combine dynamic admission charges,
  preserving wire bytes, canonical rejection and resource budgets. Matched local
  measurements show substantial blob/text decoding gains, with a small
  empty-blob regression; see [performance evidence](docs/performance.md).
## [0.4.3] - 2026-10-18

- Refresh the maintainer handoff against released 0.4.2's Linux/macOS acceptance.
  Retire the superseded CDK-companion proposal and identify the remaining
  independent tooling-adoption PRs without changing runtime behavior.
- Consolidate release receipts into the main Shared Tooling snapshot at reviewed
  0.24.2 source. Retire the supplementary snapshot and its separate integrity
  dispatch; use canonical grouped-formatting defaults. Preserve release receipts,
  selected files and recovery policy ([#9](https://github.com/dragginzgame/ic-wire/issues/9)).
- Use published IC Build 0.17.3 for retained evaluation builds and compiler
  diagnostics. Supply complete environments and explicit child custody; retire
  the Git pin and its older Host dependency family.

## [0.4.2] - 2026-10-18

- Support transparent finite `Box<T: Wire>` values with static heap admission.
  Preserve canonical frames and TypeScript layouts; recursive/unbounded schemas
  remain unsupported by the finite API. Boxing can reduce Rust enum storage at
  the cost of payload allocation; it does not promise Wasm or instruction savings.
- Select Rust 1.99.1 for development and CI; retain the Rust 1.91 library MSRV
  and original compiler identities on historical measurements.

## [0.4.1] - 2026-10-18

- Share TypeScript type rendering between finite schemas and consumer-supplied
  native registries. Host generators can emit typed method modules while keeping
  reflection, endpoint contracts and scalar checks at their consumer.

- Replace the custom Dragginz license with the standard MIT License for all
  workspace packages, including published runtime and derive crates.

- Adopt published IC Platform 0.1.9 for Principals, raw argument-size and
  stable-page checks, byte-exact traps, replies and rejects, removing the direct
  ic0 dependency. Preserve reply append/finalization order and UTF-8 reject bytes.
  Direct dependency checks now run in CI.
  CDK entrypoints/system helpers remain an evaluation-only exception under
  [#1 — Support the CDK call and entrypoint boundary](https://github.com/dragginzgame/ic-platform/issues/1).

- Adopt Shared Tooling 0.24.0 with dependency-role labels and matching grouped
  formatting and release metadata checks. Dependency selections are unchanged;
  role labels do not provide automated deployment exclusion enforcement.
- Isolate production codec diagnostics from the extracted endpoint-adapter fixture.
  Keep compiler diagnostics at IC Build and include adapter qualification in CI.
- Correct current ownership guidance and archive superseded migration handoffs;
  preserve historical reports without presenting retired replay or codec-selection
  experiments as active workflows. Runtime APIs and wire bytes are unchanged.

## [0.4.0] - 2026-10-17

- Adopt Shared Tooling 0.23.0, retaining the unmerged release-receipt helper under
  its own pinned snapshot. Select IC setup/checks explicitly and adopt ICP CLI
  1.7.0. Evaluation hosts use IC Host Artifacts/FS 0.27.1; runtime dependencies
  and wire contracts are unchanged.

- **Breaking:** remove the evaluation-only `adapter-fixture` Cargo feature and
  query/update attributes from `ic-wire-derive`. Workspace qualification now uses
  the unpublished `ic-wire-adapter-fixture` package; external callers must supply
  their own endpoint adapter. Canic remains the production macro owner.
- Stream deterministic conformance mutations from JSON Lines and share enum
  expansion across unit/newtype variants. Wire bytes, schemas and runtime APIs
  stay unchanged; no retained-data conversion or canister reset is required.

- Share the benchmark sampling/rejection path and native frame-budget calculation,
  preserving typed replies, admission order and canonical bytes.
- Consolidate fresh evaluation report identities under `provenance`, with bounded
  streaming file hashes and one compiler/server probe in the build driver. Browser
  reports become final only after retained-build verification. Historical evidence
  keeps its original format. Evaluation hosts accommodate Testkit 0.52 startup.

- Move browser fixture lifecycle to IC Testkit, compiler diagnostic mechanics to
  IC Build, and atomic release receipts to Shared Tooling. IC Wire keeps its
  test contracts, compiler recipes and release policy; upstream contributions
  are selected by immutable Git revisions.
- Bound browser fixture shutdown through forced termination and require every
  expected compiler diagnostic after a passing control. Release receipt checks
  reject symlinks and exact-byte tampering while preserving prior evidence on
  failed writes. Runtime APIs, wire bytes and package versions are unchanged.

## [0.3.5] - 2026-10-17

- Retire copied IcyDB SQL contract projections; actual endpoint and conversion
  qualification stays in [IcyDB #372](https://github.com/dragginzgame/icydb/issues/372).
  Retain generic codec and cross-language conformance coverage.
- Consolidate native scalar forwarding while preserving admission accounting,
  allocation-sensitive handlers and canonical wire bytes.
- Use IC Build's retained evidence export for evaluation artifacts, replacing
  local copy/hash/provenance plumbing. Select an immutable upstream revision
  pending [IC Build #58](https://github.com/dragginzgame/ic-build/issues/58)
  delivery; evaluations retain recipe, source custody and measurement ownership.

## [0.3.4] - 2026-10-17

- Add bounded native Serde payload/frame APIs with borrowed encoding, exact sizing
  and canonical decoding, sharing the existing Postcard envelope and output logic.
  Consumers retain explicit limits and schema/domain ownership.
  [IC Wire #5 — Native IcyDB admission](https://github.com/dragginzgame/ic-wire/issues/5).
- Include the shared native TypeScript codec from
  [PR #1](https://github.com/dragginzgame/ic-wire/pull/1), with recursive metadata,
  tuples, newtypes and sparse-array refusal. Support strict indexed-access checks
  without consumer patches ([#3](https://github.com/dragginzgame/ic-wire/issues/3)).
- Preserve TypeScript scalar checks through named aliases and bound text scratch
  buffers by the string's UTF-8 maximum instead of the remaining frame allowance.
  Concrete values no longer enumerate the type registry during alias resolution.

## [0.3.3] - 2026-10-17

- Simplify the browser evaluation to one canister and generated-codec transport
  checks. Remove fixture receipt caching, expiry/audience policy and peer retries;
  retain runtime benchmarks and the asynchronous macro proof. Adapt evaluation
  installation to the selected Testkit 0.51 API.

- Add `Bytes::into_vec` and `Text::into_string` so consumers can recover owned
  buffers without copying or allocating. Wire bytes and admission limits are unchanged.

## [0.3.2] - 2026-10-17

Pending PR integration; these changes are not yet on main.

- Share the TypeScript codec with native recursive contracts through concrete
  metadata, tuples, newtypes and explicit traversal limits. Reject sparse arrays
  before encoding and retain consumer-owned scalar validation.
  [PR #1 — Shared native TypeScript codec](https://github.com/dragginzgame/ic-wire/pull/1).
- Add an unpublished CDK endpoint companion that authorizes before argument
  sizing/copying and traps on response-encoding failure to roll back current-message
  writes. Qualify the installed endpoint through the shared build pipeline with
  retained build identities and stable-memory rollback evidence.
  [PR #2 — Bounded CDK endpoints](https://github.com/dragginzgame/ic-wire/pull/2).

## [0.3.1] - 2026-10-17

- Upgrade the derive parser to Syn 3 while retaining the Rust 1.91 floor and
  existing wire format. Preserve safe-handler admission in the macro fixture.
- Add `make publish` for explicit crates.io delivery of the runtime and derive
  packages, separate from Git release commands.
- Qualify IC Build 0.13 and Host Artifacts 0.25 in the evaluation hosts, including
  installed runtime, macro and browser trials. Keep these dependencies dev-only.

## [0.3.0] - 2026-10-17

- Prepare the first crates.io publication of `ic-wire` and `ic-wire-derive`, with
  self-contained runtime documentation, registry dependency requirements and
  package metadata. Keep evaluation tooling unpublished. Publication is separate
  from the maintainer-operated Git release.

- Retire historical canister benchmarks and use IC Build artifact records for
  maintained installed evaluations. Preserve saved measurement provenance.
- Relocate durable receipt-policy qualification to Relay; keep IC Wire’s browser
  fixture volatile and retire `evaluate-replay`. See [Relay #1 — Own durable
  receipt-policy qualification](https://github.com/dragginzgame/ic-relay/issues/1).

- Retire the four-codec screen and its private fixed/CBOR implementations; retain
  historical evidence and principal-byte nesting in production conformance.
  Remove screen-only runtime dev dependencies. Adopt Testkit 0.50 host startup
  and installation, and replace source-mutating diagnostic scripts with explicit
  pass/fail Rust fixtures and one locked offline runner.

- Retire the private typed-journey codec and Make target; qualify exact retry
  bytes and transport/application error separation against the production runtime.
  Use published Host artifact inspection in all three Wasm-reporting fixtures,
  with explicit artifact bounds and unchanged local export/measurement policy.

- Add an offline, explicitly invoked libFuzzer runtime harness and Make target,
  with bounded nested/float inputs, canonical round-trip assertions and sanitizer
  checks. Keep fuzz dependencies dev-only and instrumented builds isolated.

- Add exact request/reply frame-length helpers without output allocation,
  source-matched bounded IcyDB success fixtures and Rust/TypeScript differential
  mutation checks. Add offline inspection through generated contract modules;
  see [consumer qualification](docs/consumer-fixtures.md) for scope and limits.

## [0.2.2] - 2026-10-16

Historical implementation details remain in Git history.

## [0.2.1] - 2026-10-16

- Include the runtime benchmark modules in the disposable formatting-hook fixture,
  fixing `check-tooling` failures caused by missing source files.

## [0.2.0] - 2026-10-16

- Isolate the patch-release test changelog from the real pending release, so
  minor/major release notes do not prevent validation/recovery checks from running.

- Add finite f32/f64 wire wrappers and matching TypeScript number projections,
  preserving finite bits while rejecting NaN and infinity. Qualify scalar edges
  and bounded SQL-shaped pages with Rust/TypeScript vectors. Recursion remains
  unsupported and sibling/storage contracts are unchanged.
- Add float metadata and make the public Schema enum non-exhaustive. Downstream
  schema matches must include a fallback; this source API change selects a minor
  release. Existing wire layouts remain unchanged. Workspace versions stay at
  0.1.6 until maintainer release preparation.

- Add evaluation-only Postcard query/update attributes and a local Canic-shaped
  fixture, without changing siblings or the default runtime API. Check real Wasm
  exports, sync/async handler types, stage ordering, typed errors and rollback
  when response encoding fails after a local state change. Keep macro/auth/retry
  policy consumer-owned and preserve prior evidence separately.

## [0.1.6] - 2026-10-16

- Move the compile-fail gate from the retired private prototype to production
  Wire/Method contracts, including synchronous and async consumer-shaped handler
  bindings. Keep Canic macro ownership and runtime policy with the consumer.

- Generate typed, bounded TypeScript codecs from the same explicit Rust Wire
  metadata and Method envelope. Add cross-language scalar/composition vectors
  and malformed-input checks. Keep generation optional and transport/auth policy
  consumer-owned. Qualify generated clients in Chromium against installed nested
  queries, protected updates, retained success/failure replies and peer calls.
  Retire the private browser guards and postcard-bindgen dependency; preserve
  historical evidence. Login/delegation and production rollout remain unqualified.
- Reject saturated schema resource maxima even under maximum caller allowances.
  Preserve existing wire bytes and keep workspace versions unchanged.

## [0.1.5] - 2026-10-16

- Qualify the runtime through installed nested queries, state-changing updates,
  two inter-canister deliveries, exact replay and same-contract upgrade recovery.
  Retire the private tuple replay fixture in current source; fresh fixture state
  uses a distinct contract identity. Preserve historical measurements separately.

## [0.1.4] - 2026-10-16

- Add a Postcard-only installed protected-command experiment with bounded stable
  receipts. Verify direct-caller authorization, exact replay, conflicting request
  rejection, atomic trap rollback, same-contract upgrade recovery and capacity
  refusal. Keep application failures distinct and retain their response bytes.
  This is evaluation evidence, not a production Canic replay/auth adapter.

## [0.1.3] - 2026-10-16

- Exercise the same Postcard request/reply contract through Chromium and two
  installed canisters. Validate peer replies, retain exact repeated bytes, verify
  the recipient caller and reject invalid dispatch/configuration attempts.
  This remains a stateless transport trial, not durable replay or a Canic adapter.

- Map Canic and sibling adoption boundaries against committed source snapshots.
  Separate a bounded endpoint trial from framework-wide RPC, auth, replay, hash
  and client changes; keep storage formats and sibling source edits out of scope.
  Add concrete consumer implementation/retirement steps, readiness checks and
  supporting-library boundaries, including cases requiring no codec dependency.

## [0.1.2] - 2026-10-16

- Enable maintainer-operated Git releases through Shared Tooling, with coordinated
  Cargo metadata, retained validation evidence and interrupted-delivery checks.
  Keep package publication disabled and separate from version/tag/push delivery.

- Select Postcard for controlled inter-canister calls and owned user endpoints.
  Add a private browser experiment with Rust-generated bindings/vectors, a
  bounded TypeScript client and signed Chromium/PocketIC updates. Verify u64
  precision, malformed input and certificate rejection; public API remains open.

- Generate the native experiment's scalar record serialization from one explicit
  wire-order declaration, preserving its existing request bytes. Type retained
  request/reply buffers and add compile-fail checks for invalid callers and
  unsupported fields. This remains private evaluation syntax, not a runtime API.
- Synchronize the lockfile's local package entry with the existing 0.1.1 manifest
  so locked builds succeed; no dependency versions change.

## [0.1.1]

- Add a native typed request/reply experiment with explicit wire layout,
  contract/direction checks, bounded output and retained retry bytes. This remains
  an evaluation example, with no selected runtime codec or IC endpoint adapter.
- Fix the local Make fixture's path comparison for temporary directories with
  trailing slashes or symlink aliases, as encountered by macOS CI.

## [0.1.0]

- Initialize the maintainer-authorized private `dragginzgame/ic-wire` GitHub
  repository and activate the local formatting hook; package publication remains
  disabled.
- Establish an unpublished, dependency-free Rust workspace and no_std scaffold.
- Record scope, proposed endpoint ergonomics, codec evaluation criteria and
  preliminary Canic code-size attribution. No codec or protocol is implemented.
- Adopt reviewed Shared Tooling 0.15.4 for explicit developer tool setup,
  formatting, Git hooks, snapshot verification and workspace counts. Add local
  Make checks and Linux/macOS CI; release delivery remains disabled.
- Evaluate optional codec sharing with IcyDB. Identify reusable bounded byte
  handling and preserve database-specific format requirements; sharing remains
  a candidate to measure, with no codec or adoption selected.
