# Wire contract qualification

This unpublished workspace tests the local Wire implementation. Product packages
do not depend on it. Tests use independent byte/schema expectations and the
current Rust and TypeScript implementations, with no older Wire dependency.

| Path | Responsibility |
| --- | --- |
| `ic-wire-compatibility/` | Native, allocation and Rust/TypeScript contract tests |
| `ic-wire-consumer/` | Standalone upload workload and client generator |
| `ic-wire-instructions/` | Native instruction measurement driver |
| `typescript/` | Private TypeScript compiler, IC agent and reference transport |
| `browser/` | Explicit Chromium smoke fixture |

Package directories match Cargo package names. `wire` is the renamed dependency
on the local product crate, also exercising derive support for renamed imports.

## Schema evolution

`make test-evolution` checks sparse framing, typed recovery, schema admission,
and request and reply compatibility against emitted manifests.
`make check-evolution-typescript` uses the locked TypeScript compiler and Node
to check and exchange versioned frames with generated clients. Install the
compiler with `npm ci --prefix qualification/typescript --ignore-scripts`.
The tests cover three revisions of inline request and reply types, nested
records and tuple tails inside containers, required additions in each method
direction, and a malformed known value inside a branch eligible for recovery.
The Rust fixtures use the ordinary encoding API for absent and present
extensions and exchange frames with the generated TypeScript clients.
[Evolution contract](../docs/design/evolution.md)

## Candid comparison

Run the optional native matrix to print byte lengths for matched values and
verify that each codec decodes its own bytes back to the input. Rows compare a
Wire payload including its sparse root (add 11 bytes for a Wire frame) with a
complete single-argument Candid message including its type information.
The numeric list cases exercise
both short and maximum-width Wire varints.
Identity comparisons use Platform Types on Wire and Candid principal/account
values with matching fields. Checks distinguish absent and explicit-zero subaccounts.
Linux CI runs this target. Ragged lists keep the same row lengths across small
values, one maximum per 16 elements, and all maxima.
Finite-float rows compare ordinary Wire and Candid lists with the Wire finite
adapter at byte- and element-prefix boundaries. They check the decoded bits,
including negative zero and subnormals, and qualify non-finite rejection separately.
Signed and unsigned 128-bit rows include values beyond the 64-bit range.
Sparse-reading rows compare optional slots with an explicit slot count and
ordered (index, value) pairs. They retain present zero and trailing empty slots.
UTF-8 rows hold byte length constant for one- through four-byte characters at
the 128- and 16,384-byte length-prefix transitions.
Repeated-label rows compare inline text with a per-message table of distinct
labels and ordered u16 indices. Sizes include the complete table and references.
Rows vary message length, distinct label count and label width independently
where possible, including the transition from index 127 to 128.
Fixed-width rows compare boolean lists and 16- and 32-byte hashes in ordinary
Wire, Candid and bulk Wire. Hash size rows use 524,288 Wire nodes for both
representations. A separate 511-element check exercises the 65,536-node limit.
The structural cases cover recursive trees, maps with optional values, ragged
integer lists and success/error variants. Primitive f32 cases compare decoded
bits, including signed zero and a noncanonical quiet NaN payload.

Unit, optional-unit and nested-option lists exercise empty values, presence tags
and the 127/128 element-count boundary. For 128 units the Wire payload holds
the two-byte count and the empty extension block. Alternating absent and present
units takes 131 bytes.

The ordered-sequence rows compare a nonempty list of absolute u64 values with
its first value and unsigned gaps. Both layouts preserve duplicates. For 128
consecutive values starting at 1,700,000,000, the Wire payload shrinks from 643
to 134 bytes. The complete Candid message grows from 1,035 to 1,047 bytes.
The matrix also covers singletons, large gaps and the 127/128 gap-width boundary.

Flag bitmap rows include the logical flag count so trailing false flags survive
byte padding. At 128 flags, the bitmap reduces Wire from 131 to 20 bytes and
Candid from 139 to 42. At one flag it costs an extra Wire byte. State-run rows
compare a list of enum tags with ordered (state, count) pairs. For 256 entries,
runs of eight reduce Wire from 259 to 66 bytes and Candid from 287 to 132.
When the state changes every entry, the pairs grow to 515 and 805 bytes.
Both comparisons measure the complete payload for each consumer schema.

Three-axis point rows compare fixed `[i16; 3]` coordinates with vectors holding
the same three coordinates. At 128 points, fixed arrays use 643 Wire bytes and
vectors use 771. Each inner vector adds a count byte. Both Candid messages use
909 bytes because both coordinate types encode as `vec int16`.

Relation rows compare source-ordered pairs with a map from each source to its
ordered list of targets. They preserve repeated targets and cover sources with
at least one edge, plus the empty relation. For 16 sources starting at key
1,000,000, grouping one edge per source grows Wire from 82 to 98 bytes. With
eight edges per source, grouping reduces Wire from 579 to 258 bytes and Candid
from 785 to 354. The matrix varies source-key width and crosses the 127/128
count boundary separately for sources and targets.

```sh
cargo test --manifest-path qualification/Cargo.toml --locked \
  -p ic-wire-compatibility --features candid-comparison --test candid_matrix -- --nocapture --test-threads=1
```

The numeric distribution matrix repeats four lengths and four distributions
across signed and unsigned 16-, 32- and 64-bit integers. Compare ordinary and
packed Wire lengths for a field using its expected value distribution. Ordinary
lists pay the sum of the varint widths plus an element-count prefix. Packed
lists pay the word width times the count plus a byte-length prefix. Choose the
field representation once in the schema and preserve its element bound.

The batch rows compare individually encoded request frames with one request
frame containing the full list. Candid uses individual one-argument messages
and one list argument respectively. For 128 nested events, batching reduces
Wire from 3,456 to 1,934 bytes and Candid from 11,392 to 4,157. A one-event
batch adds overhead in both formats. Consumers can use bounded batches when
their endpoint permits shared ordering, retry and failure semantics.
For installed instructions, whole Wasm sizes and native allocations, see the
[numeric list comparison](../docs/design/performance.md#numeric-list-comparison).

## Focused checks

Run from the repository root after `cargo fetch --locked --manifest-path
qualification/Cargo.toml`. Checks use the workspace lockfile and respect Cargo’s network settings.

```sh
cargo test --manifest-path qualification/Cargo.toml -p ic-wire-compatibility --test compatibility --test codec_contracts --locked
make test-allocation
make test-consumer
make check-consumer-wasm
make test-typescript
```

The literal tests cover every u16/i16 leaf encoding, short primitive acceptance,
representative sparse payload/request/response bytes, truncated prefixes,
reusable/slice custody and borrowed input pointers. Fixed test values remain
inside an evolving root. Additional frame tests cover nested
truncation/trailing spans and nonfinite or partial packed float words at first,
middle and last positions. Product tests independently cover precise admission
boundaries and malformed-input precedence.

The [standalone consumer](ic-wire-consumer/README.md) checks a bounded upload contract
against literal frames. Its native and SDK-free Wasm workload uses only the
current codec. Optional installed measurements use a
[native driver](ic-wire-instructions/README.md).

`make test-compatibility` and `make lint-compatibility` select the contract-test
and standalone-consumer packages with default features. They do not enable
TypeScript or start a simulator. The root Makefile admits the tooling snapshot and includes both
workspaces in formatting/dependency commands. All three qualification packages
share this workspace’s catalog, lockfile, profiles and common lint policy.

## Allocation qualification

The `allocation` executable runs nine groups serially without the concurrent
Rust test harness. Published `stats_alloc` wraps the system allocator; the crate
forbids unsafe code. Fixture preparation and assertions are outside measured
regions, and outputs stay live until statistics are read.

Checks cover zero-allocation sparse sizing and preflight refusal, fallible output reservation
between passes, reused/slice output, automatic scratch admission, field
rejection before owned allocation,
borrowed spans, successful allocation controls, failed second-pass cleanup and
domain boundaries. A valid prefix can allocate before a later invalid field.
Text allocates owned bytes before UTF-8 validation, and custom Serde can allocate
independently. These are selected admission contracts, not a process heap cap.

## Rust/TypeScript exchange

`make test-typescript` requires explicitly installed Node supporting
`--experimental-transform-types`. The harness emits the current bundled runtime
and generated clients into a unique temporary directory, executes assertions,
checks returned bytes/values in Rust and removes the directory. It covers literal
records, recursive/shared schemas, large bytes, packed/ordinary fields, domain
adapters, platform identities, scalar/custom checks, malformed inputs and budgets.
Short u16/i16 and boolean inputs are checked against independent protocol rules.

For strict generated-client checking, prepare the private TypeScript compiler:

```sh
npm ci --prefix qualification/typescript --ignore-scripts --no-audit --no-fund
make check-typescript
```

The lock selects TypeScript 5.9.3. Checks enable strict mode, unchecked index
access and exact optional properties, targeting ES2022 modules. Positive and
expected-error fixtures exercise generated types. These commands do not claim
browser-engine coverage or identical Rust/JavaScript budget accounting.

The retained large-byte fixture was adapted from source revision
`2dc434b10bec2fca92345d940dbfedc05993e9d5`, paths
`crates/ic-wire/examples/typescript_conformance/large_bytes/{mod.rs,checks.mjs}`.
Their SHA-256 values are `6033c1eb16dcb25eb3fba0b0bddfa7fffb8850a26e90b4e2a8035a3a7151ae07`
and `a9e511850e6325164d948d31e1ca5283618dc514df9e31a4134d0cb351a063e7`.
Packed-i16 distributions came from
`qualification/ic-wire-benchmarks/benches/fixtures/packed_i16/mod.rs` at that
revision, SHA-256 `e2f873f88d0e190652ae43e51ff88507689f42843b85aca0d6bb3202fd102f22`.
Only workload patterns and boundary ideas are retained, under the root licence.
These provenance records introduce no external dependency or comparison gate.

## Measurement and distribution

[Performance scope](../docs/design/performance.md) describes current-only opt-z/
opt-3 Wasm and installed IC counters. They are separate from correctness checks.
`make check-packages` verifies extracted archives with the real consumer;
`make check-browser` executes generated clients in an explicitly selected
Chromium binary. See [release preparation](../docs/release.md) for prerequisites
and delivery boundaries. Neither command publishes or migrates consumers.

## Raw service round trip

From the repository root, install the private JavaScript dependencies and point
the runner at a Testkit-prepared PocketIC 16.1.0 executable.

```sh
npm ci --prefix qualification/typescript --ignore-scripts
POCKET_IC_BIN=/absolute/path/to/pocket-ic make check-raw-service
```

The [guest](ic-wire-consumer/examples/raw_service.rs) owns the counter and the
two IC exports. The [generator](ic-wire-consumer/examples/generate_service.rs)
writes both codecs, their service bindings and the bundled runtime. The
[transport](typescript/agent-transport.ts) uses the supplied HTTP agent for
queries and certified update results. The [Node check](typescript/check-service.mjs)
uses a local root key, reads the counter, adds 128, and reads the changed state.
Four invalid update frames must trap and leave the counter at 128. They cover a
missing payload, a missing extension block, a different method ID and a response
direction. Testkit starts and closes the simulator for this run.

## Generated inputs and fuzzing

Run the three proptest properties with `cargo test --manifest-path qualification/Cargo.toml
--locked -p ic-wire-compatibility --test compatibility properties::`. They exercise
arbitrary bytes, all frame directions, resource bounds, packed round trips and
float bit patterns. The TypeScript test
`generated_boolean_payloads_agree_with_the_browser_decoder` compares decoded
values and error categories for the same randomly generated bytes. Rust's
trailing-input error maps to the browser's malformed-input category.

The compatibility suite also generates nested values for two schema revisions.
It checks that readers default missing record fields and tuple tails, skip
unknown additions, and preserve shared values. Generated recovery cases check
that an unknown case drops the closest optional branch and malformed known data
still fails. Set `PROPTEST_CASES` for longer runs of either group.

```sh
cargo test --manifest-path qualification/Cargo.toml --locked \
  -p ic-wire-compatibility --test compatibility evolution_
```

For a coverage-guided run on the native target, seed the corpus with an empty
list and an empty packed field followed by a sparse block in a request frame. Keep the corpus for
subsequent runs. The following uses the pinned compiler and libFuzzer.

```sh
mkdir -p qualification/target/decode-corpus
printf 'IW\007\000\000\000\000\000\000\000\000\000\000\000' > qualification/target/decode-corpus/request
RUSTFLAGS='-Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=3 -Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table' \
cargo run --manifest-path qualification/Cargo.toml --locked -p ic-wire-compatibility \
  --features fuzz --example fuzz-decode --target "$(rustc --print host-tuple)" -- \
  -runs=20000 -max_len=80 qualification/target/decode-corpus
```

## TypeScript feature growth

Run `node qualification/measure-typescript.mjs` from the repository root. It
counts physical source lines in the TypeScript feature directory (including
its optional projections and bundled runtime) and in the remaining product
codec source, excluding unit-test files. It times two native library
checks with separate empty target directories, first codec-only and then with
`typescript` enabled. Timings include dependencies, vary by host and use the
local filesystem cache. Recheck on the same host when adding generator code.

On x86_64 Linux with Rust 1.100.0, the 2026-11-24 run counted 1,879
codec lines and 2,967 TypeScript-feature lines. The clean library checks took
1.81 s and 1.98 s respectively. These are single-host observations.
