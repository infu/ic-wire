# Shared encoding preflight and output custody

Typed and explicit projection APIs share the bounded Postcard adapter.
The public message always has a sparse extension block. This document owns
output custody and the measured one-byte shortcut.

Source provenance: `2dc434b10bec2fca92345d940dbfedc05993e9d5`,
`crates/ic-wire/src/native/encode/mod.rs` and `native/mod.rs`.
The encode adapter's source SHA-256 is
`6314ad42b620168133a95038e1539697ca7965f63f7137d2a0a9fc092a0df035`.
The initial reuse was limited to the bounded Serialize/Serializer/compound adapter
structure and admission visits, retaining the IcyDB attribution and root license.
The measured integer decision below adds only a one-byte shortcut; wider varints
and enum tags remain delegated to Postcard.

## One-byte integer shortcut

Postcard can construct a varint buffer even during sizing. One shared macro handles signed/unsigned 16/32/64-bit values:
signed values use the standard ZigZag mapping, unsigned values below 128 go
through Postcard's raw-u8 method, and all larger values use Postcard's unsigned
writer. Raw 8-bit values, 128-bit values, lengths and enum tags are unchanged.
The private adapter is constructed only around Postcard serializers, where this
one-byte identity holds; it is not a general-purpose Serde representation change.

The shortcut does not write continuation bytes or maintain a second sizing
serializer. The original value is admitted before this dispatch, so narrowing
the internal write method changes neither visits, charges, depth nor allocation
ordering. Exact i16/u16 exhaustive vectors, wider boundaries, all output
destinations, inconsistent-Serde cases and budget sweeps pass.

The shortcut is limited to this one-byte identity. Wider varints and enum tags
remain delegated to Postcard. Historical comparison runners have been retired;
[performance measurement](performance.md) covers current workloads.

## Contract

- A frame subtracts 11 from both byte and allocation allowances. The
  completed length adds the header back within the admitted byte cap.
- Envelope preparation, bounded sizing and final Vec writing use the
  same admission meter. Owned output reserves fallibly after sizing.
  Reused output is empty on error.
- Caller-slice encoding visits the projected value once after preparation.
  A destination shortage reports `Limit(Output)`. A write may modify the
  destination before returning an error. Only the successful prefix is
  publishable.
- `projected_len` measures an already-staged projection. Staging work uses
  its own allowance when the measurement occurs independently.
- Custom Serialize must be deterministic across passes. Growth and shrinkage
  between sizing and writing fail within the admitted output. Supplier errors
  map to the recorded limit after a refusal, otherwise `Malformed`.
  Output reservation failure maps to `Allocation`.

Tests retain private literal checks for the underlying Postcard fields and
qualify the public sparse-message APIs independently. See the
[qualification workspace](../../qualification/README.md) for allocation checks.
