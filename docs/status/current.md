# Current handoff

## Released 0.7.18; pending SDK integration APIs

This batch starts from `c748379325266d7fa614fefee9d745eb4a88acca` (0.7.18).
Package versions, dependency selections and the Shared Tooling snapshot remain
at that release. The lockfile adds only the existing workspace Clap dependency
to the unpublished benchmark package. The preceding tooling-adoption handoff
is retained in [its source revision](https://github.com/dragginzgame/ic-wire/blob/c748379325266d7fa614fefee9d745eb4a88acca/docs/status/current.md).

Pending 0.7.19 adds caller-owned payload/framed output slices and
`native::with_encoded`, which selects scratch or owned output before its second
serialization pass and drops owned input before synchronous consumption.
Admission, existing errors and existing framed bytes remain unchanged.
Second-pass failure can leave partial storage; consumers publish only success.

TypeScript payload codecs and host generation now match Rust's existing
unframed entry points, including zero-byte unit values and method-name dispatch
without an IC Wire method ID. They share the existing schema traversal and
checks. Framed identity/direction checks are unchanged. Consumer wrappers own
domain scalar conversion and transport; see [SDK integration](../sdk-integration.md).

## Qualification and boundaries

The [focused study](../evidence/sdk-output-0719/README.md) retains matched wasm32
instructions, uncompressed Wasm/code sizes, host generator timing, controls and
regressions. Its isolated API wrappers and mixed generic fixture have separate
measurement scopes; these are codec measurements, not a complete SDK endpoint
or runtime qualification. Wasm capacity is not live allocation or RSS.

Focused native/borrowed/runtime tests, Rust doctests (including the scoped-borrow
compile failure), strict TypeScript generation/execution and native/wasm32 MSRV
checks pass. Payload tests include Rust/TypeScript byte equality, malformed and
trailing input, admission, output ownership and consumer scalar wrappers. The
existing framed conformance and simulated big-endian checks also pass.

The [existing benchmark suite comparison](../evidence/sdk-standard-0719/README.md)
now covers all 21 maintained cohorts plus hotspots: 1,208 overlapping case
entries at opt-z and opt-3, wasm32 only. All 13,590 Wire IC rows and Candid
controls match 0.7.18 exactly; native operation allocation/heap metrics are unchanged
and all 88 Wasm artifact pairs are byte-identical. Native timings and observed
slowdowns remain separate. Both variants pass `make check-benchmarks`.
The original entry points exercised here do not qualify the new APIs' costs.
The maintained IC driver repeats identical queries; the exact-binary probe
observed one Wasm execution for three returns, not three independent executions.

No release, publication, external deployment or consumer migration is part of
this batch. Hosted CI and macOS were not run locally. Consumers still qualify
endpoint admission, publication after mutation, lifecycle behavior and exact
retained retry bytes.

The [runtime study](../evidence/runtime-0716/README.md) and
[encoder follow-up](../evidence/encoder-followup-0716/README.md) retain their
original source/dependency identities and results. They do not qualify this
batch or its current dependency graph. The single production Serde/Postcard
codec, consumer-owned custom-Serde normalization and explicit bulk-field
adoption remain the contract.
