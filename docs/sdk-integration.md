# Integrate an SDK without a codec fork

IC Wire owns the Serde/Postcard traversal, byte encoding, resource admission and
client codec. The consumer owns endpoint names, transport, authentication,
execution, rollback, scalar domain rules, persistence and retries. The APIs below
add storage and framing entry points to that same codec; existing framed bytes,
method identities and admission rules remain unchanged.

## Caller-owned output

`native::encode_to_slice(value, limits, storage)` writes a payload into a borrowed
slice. `encode_request_to_slice` and `encode_response_to_slice` include their
ordinary 11-byte envelope. They preflight admission and encoded length, check the
destination before writing, then serialize under a fresh budget into exactly the
admitted span. The returned slice borrows storage independently of the input.

Short storage returns `Error::Limit` without modification. That error also covers
resource refusal, so it is not a dedicated hint to retry with more memory.
Custom serialization can fail or change between the two passes; second-pass
failure may leave partial bytes. Publish only a successful returned slice.
Output allocation avoidance does not bound a custom serializer's own allocations
or remove the conservative allocation accounting in `Limits`.

For a small scratch buffer with automatic owned fallback, use
`native::with_encoded(value, limits, scratch, consume)`. Preflight selects the
slice or ordinary owned writer once, before serialization. Both paths retain
two passes; large values do not run through per-byte inline/spill decisions.
The owned input is dropped before the synchronous callback, and encoding failure
never calls it. Callback bytes cannot escape into a returned borrow/future.

```rust
use ic_wire::{Limits, native};
let limits = Limits { bytes: 4096, allocation: 65536, nodes: 4096, depth: 32 };
let mut scratch = [0; 128];
let length = native::with_encoded(7u64, limits, &mut scratch, |bytes| {
    assert_eq!(bytes, [7]);
    // A consumer may synchronously copy these bytes into its transport reply.
    bytes.len()
})?;
assert_eq!(length, 1);
# Ok::<(), ic_wire::Error>(())
```

Keep saved retry bytes in separately owned storage. A scratch buffer or
`encode_into` buffer can be overwritten by the next operation. The SDK retains
its own rule that no memory loan or view crosses an await; these codec helpers
do not manage SDK frames or extend publication authority.

## Payload-only clients

Rust `native::encode` and `decode` handle payloads without an envelope.
TypeScript `createPayloadCodec(schema, limits, types, checks)` provides the same
entry point. It shares the framed runtime's schema traversal, primitive checks,
registry resolution, complete-input checks and bounded writer. Selection is
explicit: payload decoding never detects or strips `IW`.

```typescript
import {createPayloadCodec} from './wire-runtime.js';
const count = createPayloadCodec({k:'u',bits:64}, {bytes:32,nodes:16,depth:8});
const bytes = count.encode(7n); // Uint8Array([7])
count.decode(bytes);            // 7n
const unit = createPayloadCodec({k:'unit'}, {bytes:0,nodes:1,depth:1});
unit.encode(null);              // empty Uint8Array
```

An IC SDK can encode its parameter tuple and direct return, with the method name
selecting the contract. Constructor arguments and standalone HTTP/application
payloads can use the factory directly. Other consumers keep explicit framed
request/response APIs with their existing identity/direction checks.

Host generators use `typescript::generate_payload_registry` with the same type
map, method list, acknowledgement map and mandatory diagnostics callback as
`generate_registry`. A payload method has `name`, `mode`, request/response
schemas and byte/node/depth allowances; it does not require an `id`. Generated
objects retain `methodName`, `mode` and the four request/response codec methods.
The selected runtime module exports `createPayloadCodec`, rather than
`createCodec`. Type-only registries still use an empty method list.

`payload_method_limits(request, response)` projects Rust policy values to those
method properties, allowing zero payload bytes for unit.
`validate_payload_schema(types, schema)` checks a standalone constructor or
payload and the complete registry without generating code. The existing framed
`method_limits` still requires at least eleven bytes. Allocation is not projected
to JavaScript: equal numeric policies do not establish equal traversal charges
or bound the JavaScript engine heap. Qualify the largest values in both runtimes.

## Domain scalars and JavaScript delivery

Keep custom Rust serialization and truthful `Wire` metadata with their consumer.
Existing custom schemas, named `check` callbacks and `runtime_module` adapters
remain the extension boundary. Generated types describe wire values; a consumer
wrapper can expose its own domain types and convert before encode/after decode.
No Principal, Candid or SDK dependency is required in the runtime.

The executable [scalar wrapper](../crates/ic-wire/examples/typescript_conformance/payload/scalar-runtime.ts)
uses the documented [unsigned magnitude](unsigned-magnitude.md) bytes, a named
canonicality check and a `bigint` presentation. Its runtime-module export forwards
to the shared payload codec and supplies checks; it implements no wire parser.
Rust/browser vectors cover zero, u128 maximum and a value beyond u128. Invalid
negative, over-budget and nonminimal values fail at the consumer boundary.

An identifier can similarly use bounded byte metadata and a named domain check,
with conversion to the consumer's Principal-like object outside the codec.
Signed values can compose an ordinary sign field and a bounded magnitude; sign
rules, negative-zero rejection and representation remain consumer decisions.
Do not infer a scalar's wire bytes from its Rust field layout or silently change
an existing byte convention. Regenerate and qualify coordinated consumers when
changing a declaration.

`typescript::RUNTIME` is the single TypeScript runtime source. Compile it and
generated modules with the consumer's pinned TypeScript build; the maintained
[conformance command](development.md#standalone-typescript-qualification) uses
strict NodeNext output and executes the resulting JavaScript. Keep compiler,
JSON/schema and generator dependencies host-only. A second hand-maintained JS
runtime, browser transport or upstream endpoint SDK is unnecessary.

SDK adoption must separately check limits before argument/reply copying, trap
on response-encoding failures after mutation, preserve exact retry bytes, and
qualify installed endpoints and lifecycle behavior. Library conformance and
focused codec measurements do not establish those consumer guarantees.
