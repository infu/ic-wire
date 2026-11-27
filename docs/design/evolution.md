# Schema evolution

The sparse message format allows optional fields and open variant cases to
evolve across independent client and service upgrades. Every value is encoded
with Serde/Postcard. The format adds local numeric IDs and byte extents at
the sites that declare them. Schemas, names and type descriptions travel through
host tooling. An inter-canister call can use the same frame entry point without
negotiation or an extra round trip.

## Declare a method that can evolve

Enable `derive` for the Rust macros. Enable `api-manifest` where you also
generate TypeScript bindings and method manifests.
The Rust-only feature graph omits the schema runtime and JSON dependency.

Use `#[derive(Wire)]` on the request and reply types from the first release.
Structs and tuple structs are extensible by default. Every typed method root
writes a sparse extension block, even when it has no additions. An empty block
adds one byte to a request or reply. The deployed method name determines the
frame ID.

```rust
use ic_wire::{Error, Limits, Wire, evolution::Method};

#[derive(Wire)]
struct GetPost {
    post_id: u64,
}

#[derive(Wire)]
struct GetPostReply {
    found: bool,
}

const LIMITS: Limits = Limits { bytes: 32, allocation: 4096, nodes: 64, depth: 8 };
const GET_POST: Method<GetPost, GetPostReply> = Method::new("get_post", LIMITS, LIMITS);

fn reply(frame: &[u8], output: &mut Vec<u8>) -> Result<(), Error> {
    let request = GET_POST.decode_request(frame)?;
    GET_POST.encode_reply(&GetPostReply { found: request.post_id == 42 }, output)
}
```

A `const` method computes its routing ID at compile time.

In the next release, edit `GetPost` in place.

```rust
#[derive(Wire)]
struct GetPost {
    post_id: u64,
    #[wire(id = 1)]
    include_comment_count: Option<bool>,
}
```

The derive puts untagged fields in the positional core and tagged fields in the
extension block, ordered by ID. An absent addition takes zero bytes inside
the block. A new reader supplies `None` when the sender omits an ID it knows.
An older reader skips a new ID by length. A third release can add another
marked optional field with a fresh ID. Keep published IDs attached to their
original meaning and types. Treat changes to the core's field order and types
as wire changes. The upgrade checker catches an untagged addition to the core
when it compares the new manifest against the published one. Use a tuple
struct for positional roots and append optional tagged elements using their
one-based positions.

Typed encoders manage scratch for present entries, including their IDs and
lengths. For generated clients, register the same typed method
with `SchemaRegistry::register_evolving_method`. Give each direction a schema
alias and a browser `PayloadLimits` value. The returned descriptor can produce
a TypeScript codec and an upgrade manifest.

## Evolve a nested record

The same derive composes nested records and tuple structs through lists,
vectors, arrays, options and results. Give a nested field its own stable ID
within its record. A record takes one byte per occurrence for an empty
extension block. A Postcard list writes its count followed by the elements.

```rust
use ic_wire::{List, Wire};

#[derive(Wire)]
struct Search {
    filters: List<Filter, 32>,
    cursor: u32,
}

// V1 has enabled only. V2 adds the tagged field.
#[derive(Wire)]
struct Filter {
    enabled: bool,
    #[wire(id = 1)]
    minimum_score: Option<u16>,
}
```

The derive prepares ordinary Serde values and their extension spans with
shared scratch. The decoder projects the admitted values with the same
frame meter. A newly required named field uses `#[wire(id = N, required)]`
with a non-optional type. The decoder reports incompatibility when a required
field is missing. Tuple tails remain optional.

Use `#[wire(fixed)]` to select a fixed Serde subtree. Its Serde layout controls
every descendant and host schema generation checks that the subtree's
metadata matches that choice. The built-in Platform Types `Account` schema
declares its record fixed. For a large fixed collection, a fixed Serde newtype
around the entire list also bypasses the temporary sequence views used by
automatic projection. Use explicit projection for boxed evolving graphs,
open variants and bounded branch recovery.

## Project nested extensions and open variants

```rust
use ic_wire::{
    Encoding, Error, Limits, TextRef,
    evolution::{Evolve, ExtensionSlot, OpenSlot, Projected, decode_projected, encode_projected_into},
};
use serde::{Deserialize, Serialize};

#[derive(Evolve)]
struct Additions<'a> {
    #[wire(id = 7)]
    note: Option<TextRef<'a, 32>>,
    #[wire(id = 9)]
    seen: Option<()>,
}

#[derive(Debug, PartialEq, Evolve)]
enum State {
    #[wire(id = 1)]
    Idle,
    #[wire(id = 2)]
    Progress(u16),
}

#[derive(Serialize, Deserialize)]
struct Event<'a> {
    sequence: u64,
    #[serde(borrow)]
    state: OpenSlot<'a, State>,
}

fn example(limits: Limits) -> Result<(), Error> {
    let mut output = Vec::new();
    let mut state_bytes = [0; 8];
    let mut extension_bytes = [0; 40];

    let additions = Additions {
        note: Some(TextRef::new("in progress")?),
        seen: Some(()),
    };
    encode_projected_into(&mut output, Encoding::Payload, limits, |ctx| {
        let core = Event {
            sequence: 42,
            state: State::encode_open(Some(&State::Progress(25)), ctx, &mut state_bytes)?,
        };
        let block = additions.encode_extensions(ctx, &mut extension_bytes)?;
        Ok(Projected::new(core, block.into_raw()))
    })?;

    let (sequence, state, additions) =
        decode_projected(&output, Encoding::Payload, limits, |event: Projected<'_, Event<'_>>, ctx| {
            Ok((
                event.core.sequence,
                State::decode_open(event.core.state, ctx)?,
                Additions::decode_extensions(ExtensionSlot::from_raw(event.extensions), ctx)?,
            ))
        })?;
    assert_eq!(sequence, 42);
    assert_eq!(state, Some(State::Progress(25)));
    assert_eq!(additions.note.unwrap().as_str(), "in progress");
    assert_eq!(additions.seen, Some(()));
    Ok(())
}
```

The two declarations are the source for typed field/case dispatch, ordering, and
generated schema IDs. The outer DTO carries explicitly typed slots. Keep the
extension slot last. Use `#[wire(id = N, required)]` with a non-optional
Rust field when every writer must supply it. A new required request field
breaks callers built against an earlier schema. An optional unit field preserves
the distinction between absence and a present zero-byte body.

The extension and open slots work inside nested Serde DTOs. A projection walks
the DTO, visits its required root block and calls the appropriate typed helpers, and advances the shared context
into nested values with `nested`. Known extension and case bodies use that
same operation meter. Include typed custom Serde work and any allocations made
by the application projection in the consumer's limits. Capture original
input for use cases that need to retain unknown opaque values.

## Append optional tuple slots

Put the extension slot at the end of a Rust tuple struct. Use an empty
extension declaration in the first revision if there are no optional tail
slots yet. Later revisions can append to that declaration while keeping the
same positional core.

```rust
use ic_wire::{
    TextRef,
    evolution::{Evolve, ExtensionSlot},
    typescript::Wire,
};
use serde::{Deserialize, Serialize};

#[derive(Evolve)]
struct PageTail<'a> {
    #[wire(id = 3)]
    continuation: Option<TextRef<'a, 32>>,
}

#[derive(Serialize, Deserialize, Wire)]
struct Page<'a>(
    u32,
    bool,
    #[serde(borrow)] #[wire(extensions)] ExtensionSlot<'a, PageTail<'a>>,
);
```

Each tail ID is its one-based logical position in the full tuple. With two
fixed slots, the first tail ID is 3. Tail declarations use consecutive IDs
and optional values. The Rust extension helpers stage and project them under
the tuple's shared context. The generated TypeScript client exposes a
positional array of the declared arity. Use `null` for an absent optional
slot and `{ some: value }` for a present slot.

A new reader supplies absence for slots an old writer omits. An old reader
skips unfamiliar tail entries at their admitted extents. These rules apply
inside lists and other DTOs, and to extensible tuples used as framed request
or response roots. The empty extension block costs one byte. When stopping
use of a published tail slot, keep its position and type in the schema and
write absence. The upgrade checker rejects shrinking the declaration or
renumbering or retyping existing slots.

## Reuse caller-owned output

For a hot path that already has a bounded output slice, use
`encode_projected_to_slice(&mut buffer, encoding, limits, prepare)`.
The preparation closure is the same as for `encode_projected_into`. It stages
the evolving field bodies under the shared meter. The slice encoder then
writes the final DTO once. Publish only the returned slice, which includes any
selected Wire frame header. A failure can modify payload storage. A physical
capacity shortage reports `Limit(Output)` and the configured byte bound
reports `Limit(Bytes)`.

Reuse initialized output buffers when the caller already controls the
capacity. The owned Vec API is useful when the caller wants Wire to size the
output before allocating. Scratch for typed extensions and output for the
complete frame are separate borrows in either API.

For a large list, `encode_projected_to_slice_with` also supports staging one
row at a time. Its callback receives a one-shot output. Borrow the shared
meter with `output.context()` and write the message with `output.write(&Projected::new(core, fields))`.
The DTO can borrow that context while a custom Serde sequence stages and
emits each row, reusing a bounded scratch slice. Return only after the
complete DTO has been written. Postcard still owns the field representations
and Wire admits both preparation and output. The qualification fixture shows
the layout and depth calls for this pattern. [Row-local fixture](../../qualification/ic-wire-consumer/examples/evolution_contract/slice_rows.rs)

## Choose where recovery is allowed

An optional open slot decodes an unfamiliar case as absence at that slot.
Use a required-open case inside a deliberately bounded optional branch to
discard the whole branch instead. An outer branch can contain records,
lists, results or another bounded option. The closest declared recovery
boundary consumes a mark from a structurally valid unknown required case or
a missing required extension. Known siblings within and after the branch are
still checked.

A bounded option is a stock Postcard option over a byte span. It adds an
extent to present payloads, and preserves present unit as distinct from
absence. Ordinary Serde options keep their default representations.
Use `OpenSlot<'a, State, true>` and the generated `encode_required` and
`decode_required` methods for the required slot. Type the optional branch as
`RecoverSlot<'a, Body<'a>>` and call its `project` method with a closure that
checks the body under `RecoveryContext`. The TypeScript runtime performs those
checks when walking the admitted schema. Bad extents, malformed known data,
exhausted limits and failures of known values' domain checks are hard errors.
Use `decode_extensions_in` under a recovery context when an absent required
extension should drop the branch. Supply one projector per field in ascending
ID order. Use an identity projector for a Serde leaf. Project every known
nested envelope under the supplied field scope before returning the field.
For a `BranchFields` declaration with a scalar at ID 1 and a `Detail` DTO
at ID 2, the first projector returns the scalar. The second validates the
detail's nested extension block.

```rust,ignore
let branch = BranchFields::decode_extensions_in(
    slot,
    scope,
    (
        |scalar, _| Ok(scalar),
        |detail: Detail<'_>, field_scope: &RecoveryContext<'_>| {
            DetailFields::decode_extensions(detail.fields, field_scope.read_context())?;
            Ok(detail)
        },
    ),
)?;
```

This projects the known detail even if field 1 is missing. Use
`decode_extensions_in` again with the supplied scope if the detail can itself
mark a missing required extension. The borrowed slots are copyable, so the
projector can validate a nested slot and return the original DTO.

## Check upgrades from shipped metadata

Emit request/response manifests for each method from the same registry that
generates clients. In the experiment, the `evolution-upgrade` host command
compares a candidate directory against one or more recorded baseline directories.
Each directory holds the per-method manifests and a `modes.json` object mapping
endpoint to `query`, `composite_query`, or `update`. Check ordinary mode,
declared bounds and the identity as part of the contract.

```sh
cargo run --locked --manifest-path qualification/Cargo.toml -p ic-wire-consumer \
  --features evolution-qualification --example evolution-upgrade -- \
  check baseline-dir candidate-dir
```

The check runs old requests against new inputs and new replies against old
readers. It reports accepted compatibility with data loss separately from
breaking changes. Keep a reviewed history of stable names and IDs. To retire an
optional extension, retain its named tombstone in `retired.json` as
`endpoint → named extension-record definition → [{ "id": 7, "name": "note" }]`.
Previously retired IDs cannot be used for new fields.

The same rules apply at any depth. In this table, the change is to the shape
used in the indicated direction. Other deployed fields and IDs keep their
meaning and representation.

| Change in V2 | Old requests read by V2 | V2 replies read by old clients |
| --- | --- | --- |
| Add an optional tagged record field | Accepted, with absence | Accepted, with the new field skipped |
| Append an optional tuple slot | Accepted, with absence | Accepted, with the new slot skipped |
| Add a required tagged record field | Incompatible when the old writer can omit it | Accepted, with the new field skipped |
| Add a case to an open variant | Earlier cases are still accepted | Unfamiliar cases become absence at the declared optional site |
| Add a case to a closed variant | Earlier cases are still accepted if their IDs and bodies stay compatible | Incompatible if the client can receive the unfamiliar case |
| Change the positional core | Incompatible | Incompatible |

An explicit recovery boundary can accept an otherwise incompatible missing
required field or unfamiliar required case by discarding the nearest marked
branch. The checker reports that loss. The SDK supplies the two method
directions and the manifest history identifies newly added fields. A V2
client calling a V1 service needs the reverse check. For example, that client
must be able to read a reply that omits any field added in V2.

A fixed slot cannot move, change wire type or disappear across independent
upgrades. For a change in an optional field's type, assign a fresh ID and
retire the old one whenever its readers can tolerate absence. Unknown type
bodies are skipped using their admitted extents. The checker covers the
supported contract shapes and configured data bounds. Test actual frame,
traversal, memory and instruction allowances separately.

## Format notes

- An extension block is a Postcard byte span. Entries contain an increasing
  positive u32 ID, a payload extent and its Postcard value. An empty block
  costs one byte. Known readers consume each typed body exactly.
- An open case uses a u32 header whose upper bits carry the stable tag and
  whose low bit indicates a following nonempty byte span. Header zero is
  absence for an optional slot. A required slot treats zero as malformed.
- Explicit projections use scratch separate from the final output. The tail
  helpers return disjoint committed bytes and unused mutable scratch. Owned
  encoding sizes the complete projected value before reserving output.
- Ordinary fixed tuples keep their arity. An explicitly extensible tuple
  can append optional slots through its final sparse block. General Candid
  type promotion, higher-order service values and negotiation are outside
  this prototype.
