# IC Wire derive

Rust declarations for IC Wire. Use the coordinated re-export.

```toml
[dependencies]
ic-wire = { path = "../ic-wire", features = ["derive", "typescript"] }
serde = { version = "1", features = ["derive"] }
```

```rust
#[derive(serde::Serialize, serde::Deserialize, ic_wire::typescript::Wire)]
#[wire(fixed)]
struct FixedSequence { sequence: u64 }
```

A fixed nested type describes a supported Serde layout for the validated schema graph.
Ordinary enum declarations also use schema projection for their Serde layout.
Unsupported attributes fail explicitly. Fixed layouts support generics, borrowed
fields, symmetric field and variant renames, standard `serde_bytes` and qualified
IC Wire packed adapters. Renamed dependencies use `#[wire(crate = "dependency_name")]`.

The same derive works on native and Wasm DTOs. It emits the same schema trait
implementation on both targets.

Use `#[derive(ic_wire::Wire)]` on the request and reply types of an
`ic_wire::Method`. Enable `derive` for Rust and `typescript` for client metadata. Add optional fields to the same declaration with
`#[wire(id = N)]` and a stable, unused ID. Untagged fields form the positional
core. Every method root encodes an extension block, including the first release
with no tagged fields. Tuple structs use the same derive and assign each
optional tail element its one-based tuple position. The derive also projects
nested `Wire` structs and tuple structs through supported containers. Add a
required named field with `#[wire(id = N, required)]` and its non-optional
type. Check new required IDs against each shipped request and reply schema.
Choose `#[wire(fixed)]` when a nested subtree uses its ordinary Serde layout.

Use `Evolve` on the field or case declarations when projecting explicit nested
envelopes.

```rust
use ic_wire::evolution::{Evolve, ExtensionSlot, OpenSlot};

#[derive(Evolve)]
struct AddedFields {
    #[wire(id = 7)]
    label: Option<ic_wire::Text<32>>,
}

#[derive(Evolve)]
enum State {
    #[wire(id = 1)]
    Ready,
    #[wire(id = 2)]
    Waiting(u16),
}

#[derive(serde::Serialize, serde::Deserialize, ic_wire::typescript::Wire)]
struct Reply<'a> {
    sequence: u64,
    #[serde(borrow)]
    state: OpenSlot<'a, State>,
    #[serde(borrow)]
    #[wire(extensions)]
    fields: ExtensionSlot<'a, AddedFields>,
}
```

Use the generated `encode_extensions` and `decode_extensions` methods to
prepare and project the final block. `State::encode_open` accepts an
`Option<&State>`, and `State::decode_open` returns absence for an unknown case.
The helpers use the containing Wire operation's admission context. For a
required open value beneath an explicitly recovering branch, use
`OpenSlot<'a, State, true>` with the generated required-case helpers and a
`RecoverSlot` enclosing the branch. Use `decode_extensions_in` to recover
the enclosing branch when a required extension is missing. Pass a tuple with
one projector per declared field, ordered by ascending ID. A Serde leaf uses
`|value, _| Ok(value)`. A field with nested envelopes projects every known
descendant in the supplied field scope before returning the validated value.
The helper completes those checks before it can mark a missing required field.

A tuple struct can declare its final `ExtensionSlot` with `#[wire(extensions)]`
as well. The tail IDs identify one-based positions. A tuple with one fixed
slot starts its optional tail at ID 2. Assign consecutive IDs and append
new optional slots after the existing ones. Schema generation validates this
sequence. Earlier tuple writers can omit the new slots, while earlier readers
skip them and preserve the enclosing alignment. Declare the tail carrier from
the first version, including when the tail is empty.

Requires Rust 1.100.0. The derive and runtime crate are released together.
See [IC Wire](https://github.com/dragginzgame/ic-wire) for usage and qualification.
Licensed under the included Toko Studios Network Licence 1.1 (`LICENSE.md`).
