# Serde runtime and Canic integration

IC Wire has one production codec: bounded Serde traversal with Postcard encoding.
Use `Serialize` / `Deserialize` on consumer DTOs and the `native` module for
framing, resource admission and strict primitive validation. `Wire` generates
host-only metadata; it never generates a serializer. Alternative codec and
derive designs are research, not selectable runtime modes.

## One-pass decoding (0.7 hard cut)

Decode requires only `Deserialize`, validates primitive encodings while reading,
and does not serialize the decoded value again. Encoder bytes and the envelope
are unchanged. Overlong varints, invalid booleans/options, malformed UTF-8 and
multi-character char payloads are rejected; trailing bytes remain rejected.
`Error::Noncanonical` is removed; invalid representations return `Error::Malformed`.

Collections and custom Serde visitors now own normalization. For example, map
payload `02 02 14 01 0a` is accepted as `{1: 10, 2: 20}` and re-encodes as
`02 01 0a 02 14`. Duplicate map keys or set entries may collapse. Successful
decoding does not promise that re-encoding reproduces the received bytes.
Consumers must validate any required ordering, uniqueness or normalization, and
retain original immutable frames for exact retries/raw-byte signatures/hashes.
Do not reconstruct those bytes from a decoded value. No fallback mode is provided.

The private reader is adapted from Postcard 1.1.3 with minimal-varint and exact-char
checks; its MIT notice is retained in `src/native/parser/LICENSE-MIT`. Upstream
Postcard still owns encoding. No forked dependency or public parser API is added.

## Hard cut from the finite API

The former serializer-generating `Wire` derive is replaced by metadata-only
`Wire`, used alongside Serde derives. The finite `Wire` trait, `Shape`, `Method`, typed retained-byte wrappers,
root finite encode/decode functions and `typescript::generate::<Method>()` are
removed. Use ordinary Serde derives, explicit method IDs and `Limits`, retain
returned immutable frame bytes for retries, and generate TypeScript through
`Wire` and `generate_registry`. No compatibility shim or fallback exists.

This changes layouts previously controlled by `wire(id)`: record fields now
follow declaration order and enum tags use zero-based declaration indices.
Postcard primitives and the eleven-byte `IW` + little-endian u64 method ID +
direction header remain unchanged (request 0, response 1). Existing native Serde
frames are unchanged. Coordinate affected Rust/TypeScript clients and regenerate
metadata; do not replay old finite frames against a changed layout.

`Bytes<N>`, `BytesRef`, `Text<N>`, `TextRef`, `List<T,N>` and finite float wrappers
remain ordinary Serde field types, not another admission strategy. Their local
bounds supplement runtime budgets. `FiniteF32`/`FiniteF64` reject NaN/infinity and
preserve every finite bit, including negative zero. Raw Serde floats retain their
Serde behavior; consumers own domain validation.

## Native Serde payloads

`ic_wire::native` handles consumer-owned Serde types, including String, Vec,
tuples, maps and recursive enums, using the same Postcard encoding and envelope.
It does not infer static maxima or TypeScript metadata during codec execution.
Consumers own declaration order, compatible method IDs, deterministic serializers
and domain validation (including finite floats where required). Unordered maps
are unsuitable unless their serializer establishes deterministic ordering.

### Choose fields for bulk data

Use these defaults when declaring a new contract:

| Data | Default field | TypeScript shape |
| --- | --- | --- |
| Binary data with a field limit | `Bytes<N>` | `Uint8Array` |
| Binary data with operation-level limits | `ic_wire::ByteBuf` or `Vec<u8>` with `#[serde(with = "serde_bytes")]` | `Uint8Array` |
| Large homogeneous integer arrays | `Packed<T>` with `i16`, `u16`, `i32`, `u32`, `i64` or `u64` | Corresponding integer typed array |
| Lists of 32-byte hashes | `Packed<[u8; 32]>` | Flat `Uint8Array` |
| Records, general collections or small-value varints | Ordinary `Vec<T>` or bounded `List<T,N>` | Array of element values |

The bulk fields use Serde's byte-buffer hook within the same Postcard codec.
Ordinary sequences visit each element; the metadata derive does not rewrite
them. A large list of small integers may still favor ordinary varints for payload
size. `PackedI16<N>` remains available when a signed-16-bit field requires an
element bound; generic `Packed<T>` has no per-field maximum.
Choose one representation per field for all values, including empty and small
inputs; do not switch layouts according to the current value's length.

Unsigned observations beyond u128 can use `Bytes<N>` as a minimal unsigned
big-endian magnitude: zero is empty bytes, and nonzero values start with a
nonzero byte. The consumer checks numeric canonicality and converts its external
arithmetic type losslessly; generic Wire byte decoding still accepts opaque
leading zeroes. `Option<Bytes<N>>` keeps missing evidence distinct from zero.
See the [unsigned evidence guide](../../docs/unsigned-magnitude.md) for bounds,
exact payload examples, Nat conversion and generated browser handling.

This complete DTO combines the recommended fields with an ordinary collection:

```rust
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ic_wire::Wire))]
struct BulkTransfer {
    payload: ic_wire::Bytes<4096>,
    samples: ic_wire::Packed<i16>,
    chunk_hashes: ic_wire::Packed<[u8; 32]>,
    labels: Vec<String>,
}
let transfer = BulkTransfer {
    payload: ic_wire::Bytes::new(vec![42; 1024])?,
    samples: ic_wire::Packed::new(vec![i16::MAX; 1024]),
    chunk_hashes: ic_wire::Packed::new(vec![[0xab; 32]; 32]),
    labels: vec!["sensor".into()],
};
let limits = ic_wire::Limits {
    bytes: 16 * 1024, allocation: 256 * 1024, nodes: 256, depth: 32,
};
let frame = ic_wire::native::encode_request(7, &transfer, limits)?;
let decoded: BulkTransfer = ic_wire::native::decode_request(7, &frame, limits)?;
assert_eq!(decoded, transfer);
# Ok::<(), ic_wire::Error>(())
```

Operation budgets bound codec admission. Consumers additionally validate their
own sample/hash counts and semantics before encoding and after decoding. Keep
existing field-local bounds when migrating a declaration. Switching an ordinary
integer/hash list to packed data changes bytes and generated client types;
coordinate both ends and resolve retained old-layout frames before switching.
The details and byte examples below apply to that explicit adoption.

### Representation and adoption details

For opaque binary fields, use byte-oriented serialization. Byte-oriented field wrappers include
`Bytes<N>` and `BytesRef<'_, N>`. A native DTO can retain its `Vec<u8>` field and
use `#[serde(with = "serde_bytes")]` with a consumer dependency on `serde_bytes`.
Ordinary `Vec<u8>` serialization visits and admits every element; the byte path
admits the buffer in bulk. Both produce the same Postcard byte-vector payload
(for example, `[0, 127, 255]` becomes `03 00 7f ff`), but their traversal and
allocation-budget accounting differ. Qualify the chosen representation against
consumer limits. The benchmark corpus checks exact frame equivalence at 4 KiB
and 1 MiB; the byte path requires no new runtime API or encoding mode.

For bulk integer vectors or hash lists, `Packed<T>` owns a `Vec<T>` and serializes
it as a single byte buffer. Supported elements are `i16`, `u16`, `i32`, `u32`,
`i64`, `u64` and `[u8; 32]`. Runtime `Limits` bound admission; there is no per-field
const bound. Use `new`, `as_slice` and `into_vec` to wrap, inspect and recover data.

```rust
let hashes = ic_wire::Packed::new(vec![[0xab_u8; 32]]);
assert_eq!(hashes.as_slice().len(), 1);
```

This is still the same Serde/Postcard codec. Selecting this field changes ordinary
list bytes to a byte count followed by fixed little-endian words. For example,
`[32767_i16]` changes from `01 fe ff 03` to `02 ff 7f`. Coordinate both endpoint
schemas and retain exact retry bytes. Small integers can produce larger payloads.
The TypeScript feature maps integers to the corresponding typed arrays (including
`BigInt64Array`/`BigUint64Array`) and hashes to flat `Uint8Array` bytes, with a length
that must be divisible by 32. Ordinary vectors retain their current encoding.

For bulk signed 16-bit values, `PackedI16<N>` owns a bounded `Vec<i16>` and
serializes it as bytes: a byte count followed by fixed-width little-endian words.
`new` admits an existing vector without copying; `as_slice` borrows its words and
`into_vec` returns ownership. `Wire` metadata projects it to `Int16Array`, with
`N` counting elements. Rust and TypeScript decode into owned storage and reject
odd byte lengths and excess elements before allocating.

```rust
let samples = ic_wire::PackedI16::<1024>::new(vec![-32768, 0, 32767])?;
assert_eq!(samples.as_slice(), &[-32768, 0, 32767]);
# Ok::<(), ic_wire::Error>(())
```

This is an explicit field layout, not an optimization of ordinary `Vec<i16>` or
`List<i16,N>`. For `[32767]`, their payload `01 fe ff 03` becomes `02 ff 7f`;
for `[0]`, `01 00` becomes `02 00 00`. Wide values shrink, small values grow.
Switch both Rust and TypeScript schemas together, regenerate clients, and resolve
retained old frames before switching a deployed field. Framing, method identity
and exact retained-byte retry requirements do not change; no fallback reader exists.
The native budget admits the packed byte span in bulk, rather than charging one
Serde node per integer. Qualify limits for this representation. Little-endian
hosts and Wasm use a safe byte view without a packing allocation; big-endian
serialization uses a bounded temporary buffer and is not performance-qualified.

For owned byte fields, use `ic_wire::ByteBuf` (or `ic_wire::Bytes<N>` for a
field-local bound). It re-exports `serde_bytes::ByteBuf` directly, with alloc-only
runtime support on native and Wasm targets. No extra dependency or Serde field
annotation is needed:

```rust
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ic_wire::Wire))]
struct Upload {
    data: ic_wire::ByteBuf,
}
let upload = Upload { data: vec![0, 127, 255].into() };
let limits = ic_wire::Limits { bytes: 64, allocation: 1024, nodes: 16, depth: 8 };
let frame = ic_wire::native::encode_request(7, &upload, limits)?;
assert_eq!(ic_wire::native::decode_request::<Upload>(7, &frame, limits)?, upload);
# Ok::<(), ic_wire::Error>(())
```

Enable IC Wire's `typescript` feature in the host metadata generator.
`ByteBuf` owns its bytes; `BytesRef<'_, N>` supports bounded borrowing.
The direct `serde_bytes` adapter and unbounded `&serde_bytes::Bytes` remain
available with a consumer dependency on `serde_bytes`. These byte fields project to
`Uint8Array`, whereas plain `Vec<u8>` projects to `number[]`. Regenerate bindings
and qualify traversal budgets when changing that field. `Wire` does not rewrite
Serde derives. Existing `#[serde(with = "serde_bytes")]` fields also derive
byte metadata, without changing the field type or serializer. Supported inputs
are `Vec<u8>`, `[u8]`, `serde_bytes::ByteBuf`/`Bytes` and references to those types;
`Option<T>` of these inputs projects to `null | { some: Uint8Array }` (and
nested options retain each tag). Aliases and generic fields with the required
Serde adapter bounds work through their types, not their spelling. `None`,
`Some(empty)` and `Some([0,128,255])` retain payloads `00`, `01 00` and
`01 03 00 80 ff`. Fixed arrays and arbitrary custom adapters
still require explicit verified metadata. Use the standard dependency under its
`serde_bytes` name (the absolute `::serde_bytes` path is also recognized).
This uses the already-measured byte path, not a new runtime optimization: at
1 MiB, retained opt-3 measurements show 1,049,715 encode instructions versus
78,644,290 for a generic vector and 2,115,305 for Candid's byte representation.
Owned byte decoding still costs 2.37× Candid; this does not fix integer lists or
hash arrays. The repository comparison report retains inputs and build identity.

```rust
use ic_wire::{Limits, native};
use serde::{Deserialize, Serialize};
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Request { text: String, values: Vec<u64> }
let limits = Limits { bytes: 4096, allocation: 65536, nodes: 4096, depth: 64 };
let request = Request { text: "hello".into(), values: vec![1, 2] };
let bytes = native::encode_request(7, &request, limits)?;
assert_eq!(native::encoded_frame_len(&request, limits)?, bytes.len());
assert_eq!(native::decode_request::<Request>(7, &bytes, limits)?, request);
# Ok::<(), ic_wire::Error>(())
```

The two small values above deliberately use ordinary varints. Large homogeneous
integer fields should start with `Packed<T>` and measure any payload-size tradeoff.

`encode`/`decode` and `encoded_len` operate on payloads without headers.
`encode_request`/`encode_response` accept borrowed Serialize values without an
owned Deserialize requirement. Native decode accepts `Deserialize<'de>` values:
strings and explicit byte views can borrow directly from the input. Ordinary
owned callers remain supported. Borrowing
does not skip resource admission or primitive validation, and does not make
arbitrary integer sequences borrowable. The decoded value cannot outlive its input.
`encoded_frame_len` includes the 11-byte header for either direction. Retain the
returned bytes unchanged for any consumer-authorized retry; IDs alone do not
establish idempotency. Sizing does not retain a serialization for later delivery.

`encode_into`, `encode_request_into` and `encode_response_into` write into a
caller-owned `Vec<u8>` and return `Result<(), Error>`. They clear previous contents
on entry and leave the buffer empty on error. Successful calls retain capacity;
serialization failures may discard capacity. They preserve the two-pass native
admission/sizing contract and the existing frame bytes. Existing capacity never
relaxes byte limits. Oversized retained capacity is caller-owned storage, not a
measurement of this operation's allocation allowance; shrink or drop it according
to the caller's memory policy. Keep separate immutable bytes for retries before
reusing a buffer.

`encode_to_slice`, `encode_request_to_slice` and `encode_response_to_slice` encode
into caller-owned storage and return its exact successful byte slice. Short
storage returns `Error::Limit` before writing; later serialization failures may
leave partial bytes. `with_encoded(value, limits, scratch, consume)` chooses
scratch or an owned output buffer once after sizing, drops the input, then lends
complete bytes synchronously to the callback. Both retain two-pass admission and
conservative allocation accounting; custom serializers remain trusted. Publish
only successful output and keep retry bytes separately owned.

```rust
use ic_wire::{Limits, native};
let limits = Limits { bytes: 4096, allocation: 65536, nodes: 4096, depth: 32 };
let mut scratch = [0; 128];
let length = native::with_encoded(7u64, limits, &mut scratch, |bytes| {
    assert_eq!(bytes, [7]);
    bytes.len()
})?;
assert_eq!(length, 1);
# Ok::<(), ic_wire::Error>(())
```

```rust
use ic_wire::{Limits, native};
let limits = Limits { bytes: 4096, allocation: 65536, nodes: 4096, depth: 32 };
let mut output = Vec::new();
native::encode_request_into(7, &"hello", limits, &mut output)?;
let text: &str = native::decode_request(7, &output, limits)?;
assert_eq!(text, "hello");
// Once the borrowed view is no longer used, the buffer can be reused.
native::encode_response_into(7, &"world", limits, &mut output)?;
# Ok::<(), ic_wire::Error>(())
```

```compile_fail
use ic_wire::{Limits, native};
let limits = Limits { bytes: 64, allocation: 1024, nodes: 64, depth: 16 };
let text: &str = {
    let frame = native::encode_request(7, &"hello", limits).unwrap();
    native::decode_request(7, &frame, limits).unwrap()
}; // The input storage cannot be dropped while its decoded view is used.
println!("{text}");
```

Limits are explicit per operation. Native `bytes` bounds the payload or full
frame, respectively. `allocation` reserves that entire byte allowance for output
scratch before admitting value storage. The remaining allowance charges eight
times visited value sizes and text/byte lengths; node and depth accounting follows
Serde traversal, including seeds and enum wrappers. It is deliberately conservative,
not an allocator/peak-memory measurement. It follows actual values. Different borrowed/owned representations may require
different traversal allowances. Native decoding suppresses collection size hints
and admits elements before their visitors can allocate. Decode applies only its
own traversal charges; it no longer charges a second serialization pass. Encoding
has its own allowances, so successful decoding does not guarantee successful
encoding under the same limits.

Custom Serde code is trusted: work and allocations outside these visited boundaries
must be bounded by its owner. Standard/custom Deserialize allocations may abort on
allocator exhaustion; the adapter does not make arbitrary Serde allocation fallible.
The codec's own output reserve is fallible. Resource refusals are `Error::Limit`;
invalid primitive representations (including overlong integers) are
`Error::Malformed`; trailing bytes and envelope identity retain their own errors. Caller input custody is excluded. Native depth allowances must fit the
consumer's stack budget. No schema-independent performance claim is made.

The traversal adapters are adapted from IcyDB's native admission implementation;
its MIT notice is retained in `src/native/LICENSE-MIT`. IcyDB's numeric policies,
SQL semantics, endpoint roster and storage encodings are not imported.

## Consumer integration qualification

Request/response decoding checks frame size, identity and direction before calling
the payload's `Deserialize`. Input is already in caller custody: rejecting before
transport allocation/copying requires consumer integration. The native adversarial
tests count consumer visits, checking zero-byte collections against node allowances
and successful mixed-value decoding after failures. These are deterministic work
checks, not IC instruction or peak-memory measurements; custom Serde remains trusted.

`typescript::generate_registry` rejects duplicate numeric method IDs within one
registry, including query/update pairs and spellings such as `"7"`/`"07"`.
Previously those registries were accepted; assign distinct IDs and coordinate
affected clients. Method names and payload schemas are not carried in the frame.
Separate registries and direct codec calls still require consumer identity checks.

Consumers own method/schema identity assignment and coordinated deployment. Native
fields follow Serde declaration order, without field IDs; changing a layout requires
coordinated consumers. Decode does not promise canonical re-encoding of normalized
values: preserve original bytes for exact retries and raw-byte hashing/signatures.
Authentication, handler admission, rollback on response failure and retry effect
deduplication require consumer end-to-end tests. Wire's codec tests cannot establish
those transport and execution guarantees.

## Concrete native TypeScript metadata

Payload-only clients use `createPayloadCodec(schema, {bytes, nodes, depth}, types,
checks)` from the same runtime. Encoding/decoding starts at byte zero, permits a
zero-byte unit payload, and retains full-consumption and traversal checks. The
existing `createCodec` keeps its envelope, identity and direction contract.
`typescript::generate_payload_registry` emits the same typed method objects as
`generate_registry` without requiring IDs; `payload_method_limits` projects its
policies and `validate_payload_schema` checks standalone payloads/constructors.
The selected consumer runtime module exports `createPayloadCodec` and can supply
named scalar checks. These additions match Rust's existing payload APIs; no new
encoding or automatic frame detection is introduced.

`typescript::RUNTIME` accepts trusted concrete metadata through
`createCodec(method, types, checks)`, including named recursive `ref` nodes,
`tuple` items and transparent `newtype` wrappers. Consumers supply byte, node and
depth allowances for both directions. Dynamic lengths are checked before copying
or descending. Missing/cyclic aliases and sparse arrays reject. Concrete nodes
and named references can name trusted scalar checks that run before encoding and
after decoding; every check along an alias chain applies. Float metadata can
supply finite numeric `min`/`max` bounds, inclusive in both directions; the
generator rejects malformed or inverted bounds. Unbounded finite floats retain
their existing representation, including negative zero. Integer metadata bounds
must be integral numbers within ±9,007,199,254,740,991; use named consumer checks
for wider bigint constraints. This restriction applies to metadata bounds, not
the supported 64/128-bit integer payload range.

Ordinary native Serde metadata can be derived as described below. Domain validation
and endpoint identities remain consumer-owned.
Host generators can pass that normalized registry and method list to
`typescript::generate_registry(&types, &methods, "./native-runtime.js", &acknowledgements, diagnostics)`. It
emits named TypeScript types and typed query/update codec objects. The selected module must export
`createCodec`; a consumer adapter can supply its scalar validators to the shared
runtime. Custom serialization projections and domain-specific constraints stay
with the consumer.
Generation checks names, references, supported type shapes and bounded metadata
traversal; it preserves supplied wire metadata and never infers native limits.
Registry names such as `__proto__` retain their ordinary own-property meaning.
Generation does not infer codec allowances or JavaScript heap bounds. The shared runtime compiles with strict
TypeScript, `noUncheckedIndexedAccess` and `exactOptionalPropertyTypes`.

### Deriving native metadata without IDs

Keep the existing Serde derives and add `ic_wire::Wire` in the host
generator build. For shared contract packages, gate it with a local feature:
`wire-schema = ["ic-wire/typescript"]`. Enable that feature in the generator.
On Wasm targets, forwarding it keeps the runtime codec but omits Wire's metadata
module, metadata trait implementations and optional JSON dependency.
The alloc-only serde_bytes dependency remains available for runtime ByteBuf use.
`ic_wire::Wire` emits metadata implementations only for native targets. Gate calls
to `ic_wire::typescript` and consumer-owned metadata implementations with
`#[cfg(not(target_family = "wasm"))]`. A direct consumer dependency such as
`serde_json` still needs its own native target gate; Wire cannot gate it for you.
Serde implementations, frames and runtime limits are independent of this feature.

```rust
# #[cfg(feature = "typescript")]
# fn example() -> Result<(), ic_wire::typescript::GenerationError> {
#[derive(serde::Serialize, serde::Deserialize, ic_wire::Wire)]
struct Page<T> {
    entries: Vec<T>,
    total: u64,
}
let schema = ic_wire::typescript::native_schema::<Page<String>>()?;
// Put `schema` in a named registry entry or a method's request/response field,
// then call the existing generate_registry with explicit method budgets.
# let _ = schema;
# Ok(())
# }
```

In a shared DTO package, replace the unconditional metadata derive above with
`#[cfg_attr(feature = "wire-schema", derive(ic_wire::Wire))]`.
The derive generates only metadata; it does not implement or replace Serde,
require `wire(id)` annotations, or claim finite static resource bounds.
Generic declarations are projected after instantiation, such as `Page<String>`.
Metadata bounds follow field types, including associated types such as `T::Item`;
the provider `T` need not implement `Wire` unless it is itself a serialized field.
Recursive fields retain conservative parameter bounds and inline recursive
metadata remains unsupported. Serde still owns the type's Serialize requirements.
Field-level `#[serde(borrow)]` and `#[serde(borrow = "'a")]` work with `Wire`,
including bounded `BytesRef`/`TextRef` DTOs. Serde validates the lifetime names;
borrowing changes ownership, not the generated client shape or payload bytes.
These borrowing annotations also work on newtype enum variants; their tags and
payload layouts are unchanged.
Rust field declaration order and zero-based enum variant order describe the
existing Serde bytes. For example, `{ a: u8, b: u16 }` with values 1/128 emits
payload `01 80 01`; reversing the declarations emits `80 01 01`. Reordering or
changing fields/variants is a protocol change requiring coordinated consumers.
There is no Candid-style schema evolution or compatibility fallback.

Supported shapes are named/unit/tuple structs, externally tagged enums (unit,
newtype, tuple and named variants), single-field `#[serde(transparent)]` structs,
generic types/where clauses, fixed-width integers, bool, unit, String/str/Cow<str>,
Vec, slices, Option, Result, Box (including boxed slices), references, serializable fixed arrays and tuples of
one through four elements, bounded byte/text/list wrappers and finite float
wrappers. Raw floats, pointer-sized integers, maps and other
types require an explicit supported projection; they are not guessed. Apart
from container-level `transparent`/`deny_unknown_fields`, field/newtype-variant
`borrow`, symmetric field/variant `rename = "name"` and the qualified field-level
`serde_bytes` adapter, Serde attributes are rejected. This includes asymmetric
serialize/deserialize renames, `rename_all`, `alias`, skip, flatten, tagging and
custom serializers. Duplicate resulting field/variant labels reject. Renames
change browser object keys/tags only: declaration-order fields and variant
ordinals retain identical Postcard bytes. Regenerate clients and preserve the
existing browser labels when replacing manual metadata. Rust raw
identifiers retain their unescaped names in the TypeScript object.
Slices use the same length-prefixed sequence metadata as Vec, including `&[u8]`.
Use `serde_bytes::Bytes` for byte-oriented `Uint8Array` metadata; slice metadata
does not make arbitrary typed slices borrowable during Serde deserialization.

`Cow<'a, str>` projects to the same text metadata as `str` and `String`, including
inside transparent identifiers. Borrowed and owned equal strings encode identically.
Ordinary Serde deserialization produces `Cow::Owned`; use `&str` when the contract
requires a borrowed view of the input frame.

`native_schema` inlines concrete schemas and rejects recursive types or schemas
exceeding 64 levels/65,536 visited nodes with `GenerationError::Complexity`.
Use `SchemaRegistry` for finite named projection of recursive declarations:

```rust
# #[cfg(feature = "typescript")]
# fn registry_example() -> Result<(), ic_wire::typescript::GenerationError> {
use ic_wire::typescript::{SchemaRegistry, generate_registry, method_limits};
#[derive(serde::Serialize, serde::Deserialize, ic_wire::Wire)]
enum Value { Null, List(Vec<Value>) }
let mut registry = SchemaRegistry::new();
let root = registry.register::<Value>("Value")?;
let types = registry.finish()?;
let request = ic_wire::Limits { bytes: 4096, allocation: 65536, nodes: 4096, depth: 64 };
let response = ic_wire::Limits { bytes: 8192, ..request };
let mut method = method_limits(request, response)?;
method.as_object_mut().unwrap().extend(serde_json::json!({
    "name":"queryValue", "mode":"query", "id":"17",
    "request":root, "response":root
}).as_object().unwrap().clone());
let acknowledgements = std::collections::BTreeMap::new();
let module = generate_registry(&types, &[method], "./wire-runtime.js",
    &acknowledgements, |report| eprint!("{report}"))?;
# let _ = module;
# Ok(())
# }
```

Register explicit roots once and reuse their returned references for methods.
Generation always computes bulk-field advice and calls the required diagnostic
handler before returning source. CLI tools can print the report as above. In
`build.rs`, emit each warning with `println!("cargo::warning={warning}")` and
report stale acknowledgements too. CI can reject a non-clean report with
`assert!(report.is_clean())`. Exact diagnostic paths map to nonempty suppression
reasons; no optional warning feature or separate lint call is needed. See the
[bulk advice guide](../../docs/packed-fields.md#host-side-bulk-advice-and-ci).
Nested derived declarations are collected automatically, including mutual recursion
and distinct generic instantiations. Registering an already collected type under
a new root name produces an alias; duplicate names reject without overwriting.
Any registration error invalidates the collector. `finish` checks shapes, references,
alias cycles and complexity before returning the map. Generated nested names use
compiler type names and traversal order for host-local interning; they are not
stable schema hashes or method identities. Regenerate the complete registry together.
Projection callbacks must be deterministic and can run more than once: collection
rechecks encountered shapes, rejecting conflicting projections of the same label.
Inline projection remains available with its existing output and limits.

Metadata is trusted host configuration, not reflection over arbitrary serializers.
A derive cannot inspect a handwritten Serialize/Deserialize implementation in
another block: use it only alongside ordinary Serde derives on that declaration.

For custom serialization, implement `typescript::Wire` explicitly and
qualify it against both Serde directions. A domain ID stored as `[u8; 32]` but
serialized as lowercase hex must describe text, for example
`{"k":"text","max":64,"check":"networkHex"}`, not an array. The generator's
runtime adapter supplies `networkHex`; it must **throw** on anything except
64 lowercase hexadecimal characters (returning false is not a rejection).
Missing named checks fail closed. Do not omit domain validation or add an
unchecked fallback. The maintained native schema conformance fixture demonstrates
this full Rust/TypeScript contract, including invalid and missing checks.
Manual recursive projections participate through the existing
`context.describe` child calls; no new derive helper or custom serializer is needed.
Keep consumer reflection for shapes without supported metadata implementations and
keep all custom scalar validators, identities and endpoint policies. This addition
does not infer handwritten Serde layouts or change bytes, envelopes or admission.

### Projecting browser budgets

`method_limits(request, response)` validates and projects the six byte/node/depth
properties from independent `Limits`. Byte allowances include the 11-byte envelope;
values must fit JavaScript's exact integer range, bytes must be at least 11, and
nodes/depth must be positive. The same validator checks manually authored method
metadata. **Allocation is omitted:** Rust allocation admission is not a JavaScript
heap bound. Equal numeric node/depth limits do not imply equal acceptance.

The maintained cross-runtime fixtures freeze these minimum node allowances for
their concrete values (Rust encode/decode versus TS encode/decode):

| Value | Rust encode / decode | TS encode / decode |
| --- | ---: | ---: |
| u8, transparent u8, byte buffer or packed i16 buffer | 4 / 1 | 1 / 1 |
| Some(u8) or a u8 newtype | 8 / 2 | 2 / 2 |
| Record containing u8 and Some(u8) | 16 / 6 | 4 / 4 |
| Recursive enum containing a list with one empty variant | 12 / 9 | 4 / 4 |

Rust charges Serde adapter/seed visits; TS charges metadata value traversal.
For a scalar, minimum depth is 3/1 in Rust encode/decode and 1/1 in TS. Bounds
also differ for wrappers and recursive structures; there is no universal scaling
factor. Exact frame byte ceilings agree for the qualified ordinary layouts.
Decode success never guarantees re-encode under the same allowance.

Consumer qualification: project each direction from its owning constants, test
real values just below/at/above byte/node/depth boundaries in both runtimes, and
record intentional differences. Include empty/nested/optional recursion, custom
validators, malformed inputs and byte/packed fields. The reusable conformance
example emits `admission.json` and `admission-results.json` with those checks;
it does not qualify a consumer's custom Serde code, JS heap or endpoint transport.

Native byte/allocation/node/depth budgets and method/direction binding are
unchanged. Metadata does not infer allowances. JavaScript enforces its existing
byte/node/depth limits; these are not a measurement of JavaScript heap allocation
or a promise of identical traversal charges to Rust's Serde visitor.

The TypeScript encoder starts with at most 64 bytes and grows its private buffer
within the selected direction's byte ceiling. Each returned frame owns its bytes;
there is no buffer pool or extra sizing pass that repeats custom validators.
Growth copies and temporary text buffers mean cumulative allocation can exceed
the frame ceiling; this is not a JavaScript heap bound.

## Package and license

Runtime and derive crates use the MIT License. Candid and simulator dependencies
are confined to unpublished benchmarks; production remains platform-independent.

### Bulk-field advice on native hosts

With `typescript`, `typescript::lint_bulk_roots` reviews primitive lists at
consumer-selected public roots, resolving registry references per use site.
Use this with `SchemaRegistry` when acknowledgements must apply to individual
fields. Root labels are chosen by the consumer, not synthetic type names;
`/Reply/fields/field_ids/list/u32` and `/Reply/fields/other_ids/list/u32` are
independent decisions even when both fields share one interned container.
Aliases add no path segments. Recursion stops at a reference already active on
the current path; sibling references are still inspected. Traversal is bounded
to 64 levels and 65,536 visits and returns `Complexity`, never a partial report.
Renaming/removing a field or changing its primitive word makes its former
acknowledgement stale. Bounds and custom scalar constraints still need consumer
review; these paths are not full contract fingerprints. Pass inline request/error
schemas as named roots when they are not registered types.

`typescript::lint_bulk_fields` retains definition-level advice for ordinary primitive
lists that may benefit from byte buffers or `Packed<T>`. Pass a map of named
schemas and a map of exact diagnostic paths to acknowledgement reasons. Inspect
`report.warnings` and `report.stale_acknowledgements`; optionally require
`report.is_clean()` in consumer CI. Include inline method request/response roots.
This opt-in lint does not rewrite Serde or affect Wasm. Small lists may be better
left compact; preserve field bounds and custom validation when adopting advice.
`Packed<i8>` uses `Int8Array` metadata and preserves ordinary signed-byte payloads
while changing traversal to bulk admission (`[-128,127]` stays `02 80 7f`).
