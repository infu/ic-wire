# Payload admission contract

Source provenance: `ic-wire` commit `2dc434b10bec2fca92345d940dbfedc05993e9d5`,
`crates/ic-wire/src/native/{mod.rs,encode/mod.rs,decode/mod.rs}`.
Encoding and decoding use the same private meter with different visit rules.
The [encoding contract](encoding.md) records preflight and destination custody.

## Accounting ledger

Let B be configured byte allowance and A allocation allowance. A fresh meter
reserves B, refuses if A < B, and retains floor((A - B) / 8) charge units.
One unit represents eight conservative allowance bytes, not measured live heap.

| Operation | Nodes | Charge units | Depth |
| --- | --- | --- | --- |
| Decode deserializer call | 1 | size_of(visitor result) | Check current depth, root 1 |
| Decode sequence element/map key/map value/enum discriminant/newtype variant seed | 1 | size_of(seed result) | Check access depth before invoking seed |
| Decode visitor after deserializer call | 0 | 0 | Current depth + 1 |
| Decode Some/newtype body | Through next deserializer call | Through next call | Visitor depth |
| Decode sequence/map/enum access | 0 | 0 | Visitor depth |
| Decode tuple/struct variant visitor | 0 | 0 | Access depth, no extra outer charge |
| Decode borrowed/owned text or byte span | 0 | span length | No extra depth check |
| Evolution projection scope | 1 | 0 | Check the scope's depth |
| Encode bounded value | 4 | 2 * size_of_val(value), checked | Require current depth + 2 (saturating) |
| Encode child | Through bounded value | Through bounded value | Parent depth + 1 |
| Encode text/bytes | 0 | span length | No additional check |

Every failed debit sets a sticky refusal. Subsequent visits fail and errors map
to `Error::Limit(kind)`. Within one visit, an existing refusal wins, then depth, nodes and allocation in that order. A later buffer debit can still succeed; it does
not clear refusal. The message path checks the meter after preparation,
decoding, and projection, so a caught refusal still rejects the message.
Sequence and map size hints are suppressed before consumer visitors reserve.
An element's seed charge is in addition to its deserializer charge. Zero-sized
elements still consume nodes. Type sizes are target-dependent; borrowed and
owned buffers both pay conservative span charges. Custom Serde work outside
these boundaries is trusted and unbounded; consumer collection normalization
can change re-encoded bytes. Retain original input for hashes/signatures/retries.

## Framing and error precedence

`decode(input, Encoding, Limits)` admits the complete selected input.

1. Reject total input exceeding configured bytes as Limit.
2. For frames, reject fewer than 11 bytes as Malformed, then mismatched magic,
   little-endian method or direction as Identity.
3. For frames, subtract 11 from both byte and allocation allowances with checked
   subtraction; failure is Limit. Nodes and depth remain unchanged.
4. Deserialize the sparse-root structure through bounded Postcard. A failing
   operation maps to Limit after a meter refusal, otherwise Malformed.
5. Reject trailing bytes, then project known extensions and variant bodies
   through the shared meter. Missing required fields and unfamiliar required
   cases report Incompatible when the enclosing projection cannot recover.

Primitive acceptance belongs to published Postcard 1.2.0, with Serde 1.0.229.
Redundant varints remain admitted. A length-prefixed Rust `char` must contain
exactly one Unicode scalar. Multiple scalars produce `Error::Malformed`.
No reserialization/canonicality pass is introduced. Incremental decoding may
allocate a valid prefix before encountering malformed later input.

`LimitKind::Output` identifies a short destination. `LimitKind::Scratch`
identifies insufficient temporary storage in an explicit projection.
`LimitKind::Field` identifies a bound exceeded in a field constructor.
A Serde visitor rejecting invalid field data still reports `Malformed`.
Allocation failure after successful admission remains `Error::Allocation`.

Typed encoders stage extensions on the stack. A staging capacity refusal can
trigger a retry with a bounded heap buffer. Every attempt consumes the same
node allowance. After a scratch refusal, temporary views are dropped and
their allocation charges are reclaimed. Heap scratch capacity and the
successful preparation count toward the final write's allocation allowance.
Byte, traversal and final output refusals propagate immediately.

## Narrow reuse decision

Destination: private `decode/admitted.rs`, a Serde adapter around Postcard.
Reuse unit: the source `native/decode/mod.rs` visitor/access/seed adapter,
including its IcyDB adaptation attribution. License remains the root LICENSE.md.
The source SHA-256 is
`e9a8cf090e994f8e14b79aa0dba09c5ba00dacdde607786fbe94795fbf429dda`.
Its dependencies are Serde, alloc and the private meter; it owns no primitives,
transport or orchestration. Keeping the exact visits avoids an unqualified
accounting change. The measured [one-byte integer shortcut](encoding.md) changes
only internal Postcard dispatch after admitting the original value; it does not
alter this accounting ledger.
Future consolidation must preserve this ledger or obtain a migration
decision. No complexity or performance improvement is claimed for this reuse.

Qualification uses literal payload/frame cases, exact and insufficient allowances,
borrowed-pointer checks, malformed/trailing intersections and zero-sized sequence
visits. Native and Wasm compilation are separate from runtime/performance proof.
Selected literal and allocator-instrumented checks are recorded in the
[qualification workspace](../../qualification/README.md); full coverage remains open.

The codec-only Wasm graph excludes SDKs, Candid, schema generation,
`serde_json` and `serde_derive`. Postcard's cobs dependency still brings in
`thiserror-impl`, a proc macro that runs on the build host.

Bounded byte/text/list adapters now have a [field contract](fields.md) and a
record-level literal test. The [packed-field workload](packed.md) now covers the
native representative record. The [qualification workspace](../../qualification/README.md)
checks selected literal bytes, admission and custody contracts independently.
Selected allocation and Rust/TS exchange checks are recorded there. They do not
qualify all custom Serde behavior or establish a process-wide allocation cap.
