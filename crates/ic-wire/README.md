# IC Wire

Evolvable messages over bounded Serde/Postcard for Internet Computer contracts, with
optional schema collection, bulk-field lint and TypeScript client generation.
The codec is `no_std` with `alloc`, Wasm-safe and independent of IC SDKs.
Rust 1.100.0 or later is required. The examples use the pending evolution API.
Adjust the path to the checked-out IC Wire crate in your workspace.

```toml
[dependencies]
ic-wire = { path = "crates/ic-wire", features = ["derive"] }
```

```rust
use ic_wire::{decode, encode, Encoding, Limits, Wire};

#[derive(Debug, PartialEq, Wire)]
struct Reply { accepted: bool, revision: u64 }

let limits = Limits { bytes: 64, allocation: 4096, nodes: 64, depth: 8 };
let value = Reply { accepted: true, revision: 129 };
let bytes = encode(&value, Encoding::Payload, limits)?;
let decoded: Reply = decode(&bytes, Encoding::Payload, limits)?;
assert_eq!(decoded, value);
# Ok::<(), ic_wire::Error>(())
```

Use `Encoding::Frame { method, direction }` for request/response framing and
`method_id(name)` for name-derived identities. `encode_into` reuses a Vec and
`encode_to_slice` writes a derived message into caller storage. Both handle
temporary extension storage under the encoder's limits. For manual roots,
use `evolution::encode_projected_to_slice`. Decode rejects trailing bytes.
Header identity is independent of authentication.

## Fields and limits

`Bytes`, `Text` and `List` provide explicit field bounds; `BytesRef` and `TextRef`
support borrowed decoding. `ByteBuf`/`ByteArray`, `Packed<T, N>`, finite
float wrappers and `serde_packed` adapters express intentional bulk layouts.
An ordinary array and `ByteArray` have different length-prefix representations.
Selecting an adapter is a consumer schema decision, never an automatic rewrite.

`Packed<T, N>` bounds the element count for any supported packed word type;
`Packed<T>` omits the field bound. Both use the fallible `new(Vec<T>)` constructor.
Use `Packed<i16, N>` in place of the removed `PackedI16<N>`; there is no old-name
alias. Browser metadata uses one `packed` kind with a word and optional maximum.

Limits govern bytes, conservative allocation charges, traversal nodes and depth.
Allocation accounting is conservative and differs from measured heap usage.
Owned encoding sizes the message before allocating output. Custom Serde code
remains trusted and must serialize deterministically across passes. Reusable Vec
output is empty on error. Slice encoding may modify the destination before
returning an error, including when it is too short. Publish only the prefix
returned on success. Retain original input bytes for signatures, retries or
hashes because decoding and re-encoding can normalize their representation.

### Sizing a request or reply

For the two-field `Reply` above, the largest bare message takes 12 bytes.
The boolean uses one byte, the largest `u64` uses ten, and the empty sparse
block uses one. A framed reply adds 11 bytes. Traversal and allocation
allowances also account for staging and reading the typed projections.
Size each direction separately and exercise the largest valid shape on the
target, including populated optional fields. Use `projected_len` for an
already-staged `evolution::Projected` value.

### Choosing an integer list layout

For signed and unsigned 16-, 32- and 64-bit lists, use the field's element
limit for `N` in either layout.

| What matters for this field? | Starting choice | How to confirm |
| --- | --- | --- |
| Schema limits every unsigned value to 0–127 or every signed value to −64–63 | Keep the ordinary list for payload size | Each element uses one varint byte throughout that range. |
| Typical values are small, or their distribution is unknown | `List<T, N>` for ordinary varints | Compare encoded lengths for both layouts on representative inputs. |
| Most values require many varint bytes | `Packed<T, N>` for fixed-width words | Check typical and largest allowed messages against your byte budget. |
| Encoding or decoding a large list dominates IC instructions | Try `Packed<T, N>`, even for small integers | Measure encode and decode on the target canister and check the larger small-value payload fits. |

For short lists, include typical lengths in the decision. On the measured opt-z
canister, a single small `u32` used 2 bytes and 1,075 encode instructions with
`List`, versus 5 bytes and 1,121 instructions with `Packed`. At 16 such values,
`Packed` used fewer instructions for both encoding and decoding, while `List`
still produced the shorter payload.

For example, 4,096 copies of `1_u32` use 4,098 ordinary payload bytes or
16,387 packed bytes. With 4,096 copies of `u32::MAX`, the ordinary payload
grows to 20,482 bytes and the packed payload stays at 16,387. If both layouts
meet your budgets and you already ship one, keep the existing layout. Select
once per schema: switching layouts per message changes how the receiver must
decode the field.

### Floating-point lists

Keep an ordinary list when NaN or infinity is valid. For finite-only fields,
the bulk adapters preserve the bits of finite values, including negative zero.
Both layouts use four bytes per f32 or eight per f64. Their prefixes count
different units, so packing first costs an extra byte at 32 f32s or 16 f64s.
At 128 elements the ordinary count also needs two bytes and the sizes match again.

### Flags and fixed-size hashes

Ordinary boolean lists and `serde_packed::bools` both use one byte per flag
plus the count prefix. Choose the adapter for bulk traversal and boolean-byte
validation. If payload size matters, compare a bounded flag count and a
length-prefixed bitmap. In the measured Wire layout the bitmap costs one extra
byte at one flag, ties at two, and first saves a byte at three. At 128 flags
it uses 19 bytes versus 130. Specify bit order and enforce the flag bound and
exactly `ceil(count / 8)` bytes when decoding.

For a list of 16- or 32-byte hashes, `Packed<[u8; W], N>`
counts payload bytes in its prefix, while an ordinary list counts hashes.
At a field bound of seven 16-byte hashes or three 32-byte hashes, both prefixes
always fit in one byte. Packing then reduces traversal at the same payload size.
The first extra prefix byte appears at eight or four hashes respectively.

### Sparse readings and repeated labels

For mostly empty readings, compare a list of optional values with a slot count
and ordered index/value pairs. Include the slot count so trailing empty slots
survive decoding. Measure both typical and dense inputs, where the index for
each reading can outweigh the savings from omitting empty slots.

For repeated labels, compare inline text with a per-message table and a list of
indices. Include the table in the measured payload and try messages with all
distinct labels. The choice depends on label length and repetition, which the
schema alone cannot tell you.

For consecutive repeats of the same enum state, compare a list of tags with
ordered `(state, run_length)` pairs. For 256 states with runs of eight, pairs
used 65 Wire bytes versus 258 for tags. When the state changed every entry,
pairs used 514 bytes. Measure both patterns and bound the sum of run lengths
before expanding the list.

## Optional schema features

| Feature | Purpose |
| --- | --- |
| `api-manifest` | Also exports bounded per-endpoint API snapshots for CI |
| `service-client` | Generates a typed TypeScript service over existing method codecs |
| `typescript` | Schema collection, validated graph, lint and generated clients |
| `derive` | Provides the `ic_wire::Wire` and explicit `Evolve` derives |
| `platform-types` | Also projects published Platform Types Principal/Account |

Use `derive` for shared DTOs. Enable `typescript` in the builds that generate client metadata:

```toml
[dependencies]
ic-wire = { path = "crates/ic-wire", features = ["derive", "typescript"] }
```

Use the same declaration on native and Wasm:

```rust
use ic_wire::Wire;

#[derive(Wire)]
struct Request { sequence: u64 }
```

With `derive` and `typescript` enabled, the schema trait implementation is
available on both targets, including `ValidatedRegistry::from_type::<Request>`.
A codec-only guest can keep `derive` and omit `typescript`.

For API drift checks, enable `api-manifest` on the host-side generator.
Pass the deployed endpoint names and the same `Method` descriptors used to
generate clients. The map contains JSON text keyed by endpoint name:

```rust,ignore
let documents = graph.api_manifests(&[("system_get_user_authority", method)])?;
let snapshot = &documents["system_get_user_authority"];
```

Compare the generated directory with a known-good client baseline. The exporter
runs when explicitly called and writes no files itself.

## TypeScript delivery

Enable `service-client` in the generator to group mounted methods into one client.
For example, if `method_module` exported a codec named `codec` into
`profile.ts`, select the public client name and the deployed endpoint separately.
Each method module can export its own `codec`, `Request`, and `Response`. Pass
any of the validated registries to set the service module's output allowance:

```rust,ignore
use ic_wire::typescript::ServiceMethod;

let source = graph.service_module(&[ServiceMethod {
    binding: "getProfile",
    codec_export: "codec",
    endpoint: "get_profile",
    mode: "query",
    codec_module: "./profile.ts",
}])?;
```

Write the returned source to `service.ts`. Call
`createService(canisterId, transport).getProfile(request)` to receive a promise
of the codec's reply type. The generated `Transport<CanisterId>` interface
accepts the application's canister ID type. Its `query` and `update` methods
receive the deployed name and request frame and return a promise of reply bytes.
Composite queries use `query`. Codec refusals and transport failures reject the
promise. An application error variant in a valid reply resolves as typed data.

Shared error handlers can recognise a Wire codec error from any generated
runtime copy by its stable `name` and `code` properties. Check for
`name === 'WireError'` and one of the codes `'limit'`, `'malformed'`,
`'identity'`, `'value'`, or `'incompatible'`. An incompatible evolved value
cannot be represented by this reader at any applicable recovery boundary.
Each bundled runtime defines its own constructor, so `instanceof WireError`
only recognises errors from that copy.

The [executable counter example](https://github.com/dragginzgame/ic-wire/tree/main/qualification#raw-service-round-trip)
connects a generated client to a Rust canister over HTTP. Its `read` query and
`add` update both use raw Wire frames. The application supplies the agent and
its identity. The guest checks the complete request length before copying it,
decodes before updating the counter, and traps on invalid input. Both request
and reply allow 17 bytes including the header. The generator checks both method
IDs together. These raw methods have no Candid signature for Candid UI calls.

Register application roots with `SchemaRegistry`, finish into `ValidatedRegistry`,
then call `payload_module` or `method_module` with explicit `PayloadLimits`, an
acknowledgement map and a diagnostic handler. Display the bulk report or reject
it with `is_clean()`. The map accepts exact paths and review reasons. An
application exporter can also share a `BulkPolicy` across method callbacks to
acknowledge one advisory kind for a declared list element type or DTO field.
Check `policy.stale()` after the final module.
Warnings include the declared maximum element count when available. Empty-only
collections skip element advice; lists that can contain zero-byte elements still
receive traversal guidance.

Write `ic_wire::typescript::RUNTIME` to `runtime.ts` beside the generated modules
and select `./runtime.ts` as their runtime import. Compile them together with a
TypeScript toolchain supporting BigInt and typed arrays. For plain browser ES
modules, TypeScript 5.9's `rewriteRelativeImportExtensions` emits `.js` imports.
The runtime is bundled in this crate; there is no separate npm runtime to fetch.
Use the generated TypeScript shapes: wide integers are BigInt and bulk fields
use the corresponding typed arrays. Transport, authentication and retries belong
to the application.

Scalar bounds are enforced by the browser runtime. Named custom checks can be
supplied through a consumer runtime wrapper forwarding the factory's optional
third `Checks` argument. Checks are trusted synchronous functions which throw
to reject; their work is outside traversal budgets. Consumer Serde owns matching
Rust validation.

## Evolvable method types

Enable `derive` and declare `Method<Request, Reply>` with
`#[derive(ic_wire::Wire)]` on both types. Every framed root has room
for later additions. Add an `Option<T>` field to the same struct with
`#[wire(id = N)]` and a stable, unused ID. Tuples can append optional slots
identified by their one-based positions. An empty extension block costs one
byte per root, and IDs and lengths are written only for present additions.

On the host, register the typed method with
`SchemaRegistry::register_evolving_method` to derive its TypeScript codec and
method manifest. Nested derived structs and tuple structs also evolve through
lists, options, results and arrays. Their empty blocks cost one byte per
occurrence. Use `#[wire(fixed)]` when a nested value should keep its entire
Serde layout. The explicit projection API supports open variants and branch
recovery. Manifest checks compare old writes to new request readers and new
writes to old reply readers, including required fields within nested records.
[Declarations and projection](https://github.com/dragginzgame/ic-wire/blob/main/docs/design/evolution.md)

## Cutover to evolvable messages

The root Rust APIs require an evolving message. An explicit Serde projection
uses `evolution::Projected` with a sparse root block. The browser message
factories likewise require an extension record or tuple at the root. Fixed
values remain available as nested fields. Regenerate the client and runtime
together when changing from the previous positional format.

See the [repository](https://github.com/dragginzgame/ic-wire) for qualification,
design and measured tradeoffs. Smaller Wasm in the sample workload does not imply
universally faster encoding. See the included `LICENSE.md` for the licence covering this copy.
