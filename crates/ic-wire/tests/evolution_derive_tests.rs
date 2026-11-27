//! A single ID declaration drives the codec and generated client metadata.
#![cfg(feature = "derive")]

#[cfg(feature = "typescript")]
use ic_wire::typescript::{ValidatedRegistry, Wire};
use ic_wire::{
    Encoding, Error, Limits, TextRef,
    evolution::{
        Evolve, ExtensionSlot, OpenSlot, Projected, ReadContext, RecoverSlot, decode_projected,
        encode_projected_into,
    },
};
use serde::{Deserialize, Serialize};

const LIMITS: Limits = Limits {
    bytes: 1024,
    allocation: 65536,
    nodes: 2048,
    depth: 12,
};

#[derive(Evolve)]
struct OldFields {}

#[derive(Debug, PartialEq, Evolve)]
struct Fields<'a, T: Copy = u32> {
    #[wire(id = 9)]
    stamp: Option<()>,
    #[wire(id = 7, rename = "annotation")]
    note: Option<TextRef<'a, 16>>,
    #[wire(id = 2, required)]
    epoch: T,
}

#[derive(Debug, PartialEq, Evolve)]
enum OldState {
    #[wire(id = 1)]
    Idle,
}

#[derive(Debug, PartialEq, Evolve)]
enum State<T> {
    #[wire(id = 3)]
    Future(T),
    #[wire(id = 1)]
    Idle,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(Wire))]
struct NewPacket<'a> {
    id: u8,
    #[serde(borrow)]
    state: OpenSlot<'a, State<u16>>,
    suffix: u8,
    #[serde(borrow)]
    #[cfg_attr(feature = "typescript", wire(extensions))]
    extension: ExtensionSlot<'a, Fields<'a>>,
}

type PacketCore<'a, State> = (u8, OpenSlot<'a, State>, u8);

impl<'a> NewPacket<'a> {
    fn into_root(self) -> Projected<'a, PacketCore<'a, State<u16>>> {
        Projected::new(
            (self.id, self.state, self.suffix),
            self.extension.into_raw(),
        )
    }
}

fn project_core<T>(root: Projected<'_, T>, ctx: &ReadContext<'_>) -> Result<T, Error> {
    root.extensions.visit(ctx, |_, _| Ok(()))?;

    Ok(root.core)
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(Wire))]
#[cfg_attr(feature = "typescript", wire(fixed))]
struct NewBranch<'a> {
    #[serde(borrow)]
    state: OpenSlot<'a, State<u16>, true>,
    valid: bool,
}

#[derive(Deserialize)]
struct OldBranch<'a> {
    #[serde(borrow)]
    state: OpenSlot<'a, OldState, true>,
    valid: bool,
}

#[derive(Evolve)]
struct PreviousBranchFields<'a> {
    #[wire(id = 7, rename = "annotation")]
    note: Option<TextRef<'a, 16>>,
}

#[derive(Serialize)]
struct PreviousRecordBranch<'a> {
    id: u8,
    extension: ExtensionSlot<'a, PreviousBranchFields<'a>>,
}

#[derive(Deserialize)]
struct CurrentRecordBranch<'a> {
    id: u8,
    #[serde(borrow)]
    extension: ExtensionSlot<'a, Fields<'a>>,
}

#[derive(Evolve)]
struct TupleTail<'a> {
    #[wire(id = 2)]
    extra: Option<TextRef<'a, 16>>,
    #[wire(id = 3)]
    marker: Option<()>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(Wire))]
struct PreviousTuple<'a>(
    u32,
    #[serde(borrow)]
    #[cfg_attr(feature = "typescript", wire(extensions))]
    ExtensionSlot<'a, OldFields>,
);

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(Wire))]
struct CurrentTuple<'a>(
    u32,
    #[serde(borrow)]
    #[cfg_attr(feature = "typescript", wire(extensions))]
    ExtensionSlot<'a, TupleTail<'a>>,
);

#[test]
fn declared_fields_and_cases_drive_cross_revision_projection() {
    let mut state_scratch = [0; 16];
    let mut extension_scratch = [0; 16];
    let mut bytes = Vec::new();
    let current = Fields {
        note: Some(TextRef::new("hi").unwrap()),
        epoch: 128_u32,
        stamp: Some(()),
    };
    encode_projected_into(&mut bytes, Encoding::Payload, LIMITS, |ctx| {
        let state = ctx.nested(|ctx| {
            State::encode_open(Some(&State::Future(257_u16)), ctx, &mut state_scratch)
        })?;
        let extension = current.encode_extensions(ctx, &mut extension_scratch)?;
        Ok(NewPacket {
            id: 3,
            state,
            suffix: 42,
            extension,
        }
        .into_root())
    })
    .unwrap();
    assert_eq!(
        bytes,
        [
            3, 7, 2, 0x81, 2, 42, 11, 2, 2, 0x80, 1, 7, 3, 2, b'h', b'i', 9, 0
        ]
    );

    let (state, fields, suffix) = decode_projected(
        &bytes,
        Encoding::Payload,
        LIMITS,
        |packet: Projected<'_, PacketCore<'_, State<u16>>>, ctx| {
            let (_, state, suffix) = packet.core;
            let state = ctx.nested(|ctx| State::decode_open(state, ctx))?;
            let fields =
                Fields::decode_extensions(ExtensionSlot::from_raw(packet.extensions), ctx)?;
            Ok((state, fields, suffix))
        },
    )
    .unwrap();
    assert_eq!(state, Some(State::Future(257)));
    assert_eq!(fields, current);
    assert_eq!(fields.note.unwrap().as_str().as_ptr(), bytes[14..].as_ptr());
    assert_eq!(suffix, 42);

    let old = decode_projected(
        &bytes,
        Encoding::Payload,
        LIMITS,
        |packet: Projected<'_, PacketCore<'_, OldState>>, ctx| {
            let (_, state, suffix) = packet.core;
            let state = ctx.nested(|ctx| OldState::decode_open(state, ctx))?;
            OldFields::decode_extensions(ExtensionSlot::from_raw(packet.extensions), ctx)?;
            Ok((state, suffix))
        },
    )
    .unwrap();
    assert_eq!(old, (None, 42));

    encode_projected_into(&mut bytes, Encoding::Payload, LIMITS, |ctx| {
        let state = ctx
            .nested(|ctx| OldState::encode_open(Some(&OldState::Idle), ctx, &mut state_scratch))?;
        let extension = OldFields {}.encode_extensions(ctx, &mut extension_scratch)?;
        Ok(Projected::new((3_u8, state, 42_u8), extension.into_raw()))
    })
    .unwrap();
    let result = decode_projected(
        &bytes,
        Encoding::Payload,
        LIMITS,
        |packet: Projected<'_, PacketCore<'_, State<u16>>>, ctx| {
            assert_eq!(
                ctx.nested(|ctx| State::decode_open(packet.core.1, ctx))?,
                Some(State::Idle)
            );
            Fields::<u32>::decode_extensions(ExtensionSlot::from_raw(packet.extensions), ctx)
        },
    );
    assert_eq!(result, Err(Error::Incompatible));
}

#[test]
#[cfg(feature = "typescript")]
fn carrier_schema_uses_the_same_types_and_ids() {
    let graph =
        ValidatedRegistry::from_type::<NewPacket<'static>>("Packet", Default::default()).unwrap();
    let metadata = graph.metadata_json().unwrap();
    let metadata: ic_wire::typescript::__private::Value = serde_json::from_str(&metadata).unwrap();
    let root = &metadata["Packet"];
    assert_eq!(root["k"], "extension-record");
    let fields = root["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[1]["value"]["k"], "open-variant");
    assert_eq!(fields[1]["value"]["variants"][0]["id"], 1);
    assert_eq!(fields[1]["value"]["variants"][1]["id"], 3);
    let extensions = root["extensions"].as_array().unwrap();
    assert_eq!(extensions[0]["id"], 2);
    assert_eq!(extensions[0]["required"], true);
    assert_eq!(extensions[1]["id"], 7);
    assert_eq!(extensions[1]["name"], "annotation");
    assert_eq!(extensions[1]["required"], false);
    assert_eq!(extensions[2]["id"], 9);
    assert_eq!(extensions[2]["value"]["k"], "unit");

    let graph = ValidatedRegistry::from_type::<(RecoverSlot<'static, NewBranch<'static>>, u8)>(
        "WithFallback",
        Default::default(),
    )
    .unwrap();
    let metadata: ic_wire::typescript::__private::Value =
        serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    let branch = &metadata["WithFallback"]["items"][0];
    assert_eq!(branch["k"], "recover");
    assert_eq!(branch["item"]["fields"][0]["value"]["required"], true);
}

#[test]
fn required_case_markers_recover_inside_typed_option() {
    let mut state_scratch = [0; 8];
    let mut branch_scratch = [0; 16];
    let mut bytes = Vec::new();
    encode_projected_into(&mut bytes, Encoding::Payload, LIMITS, |ctx| {
        let branch = ctx.nested(|ctx| {
            let state = ctx.nested(|body| {
                body.nested(|state| {
                    State::Future(42_u16).encode_required(state, &mut state_scratch)
                })
            })?;
            RecoverSlot::write(ctx, &NewBranch { state, valid: true }, &mut branch_scratch)
        })?;
        Ok(Projected::empty((branch, 99_u8)))
    })
    .unwrap();
    assert_eq!(bytes, [1, 4, 7, 1, 42, 1, 99, 0]);

    let project = |root: Projected<'_, (RecoverSlot<'_, OldBranch<'_>>, u8)>,
                   ctx: &ReadContext<'_>| {
        let (branch, suffix) = project_core(root, ctx)?;
        let result = ctx.nested(|ctx| {
            branch.project(ctx, |branch, recovery| {
                let state =
                    recovery.nested(|state| OldState::decode_required(branch.state, state))?;
                Ok((state, branch.valid))
            })
        })?;
        Ok((result, suffix))
    };
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, project),
        Ok((None, 99))
    );
    bytes[5] = 2;
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, project),
        Err(Error::Malformed)
    );
}

#[test]
fn missing_required_fields_recover_after_checking_known_values() {
    let mut extension_scratch = [0; 16];
    let mut branch_scratch = [0; 32];
    let mut bytes = Vec::new();
    let old = PreviousBranchFields {
        note: Some(TextRef::new("hi").unwrap()),
    };
    encode_projected_into(&mut bytes, Encoding::Payload, LIMITS, |ctx| {
        let branch = ctx.nested(|ctx| {
            let extension =
                ctx.nested(|inner| old.encode_extensions(inner, &mut extension_scratch))?;
            RecoverSlot::write(
                ctx,
                &PreviousRecordBranch { id: 3, extension },
                &mut branch_scratch,
            )
        })?;
        Ok(Projected::empty((branch, true)))
    })
    .unwrap();
    assert_eq!(bytes, [1, 7, 3, 5, 7, 3, 2, b'h', b'i', 1, 0]);

    let strict = |root: Projected<'_, u8>, ctx: &ReadContext<'_>| {
        Fields::<u32>::decode_extensions(ExtensionSlot::from_raw(root.extensions), ctx)
            .map(|fields| (root.core, fields.epoch))
    };
    let recover = |root: Projected<'_, (RecoverSlot<'_, CurrentRecordBranch<'_>>, bool)>,
                   ctx: &ReadContext<'_>| {
        let (branch, suffix) = project_core(root, ctx)?;
        let result = ctx.nested(|ctx| {
            branch.project(ctx, |branch, scope| {
                let content = Fields::decode_extensions_in(
                    branch.extension,
                    scope,
                    (
                        |value, _| Ok(value),
                        |value, _| Ok(value),
                        |value, _| Ok(value),
                    ),
                )?;
                Ok(content.map(|fields| (branch.id, fields.epoch)))
            })
        })?;
        Ok((result, suffix))
    };
    assert_eq!(
        decode_projected(&bytes[2..9], Encoding::Payload, LIMITS, strict),
        Err(Error::Incompatible)
    );
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, recover),
        Ok((None, true))
    );
    let with_epoch = [1, 11, 3, 9, 2, 2, 0x80, 1, 7, 3, 2, b'h', b'i', 1, 0];
    assert_eq!(
        decode_projected(&with_epoch, Encoding::Payload, LIMITS, recover),
        Ok((Some(Some((3, 128))), true))
    );

    bytes[7] = 255;
    assert_eq!(
        decode_projected(&bytes[2..9], Encoding::Payload, LIMITS, strict),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, recover),
        Err(Error::Malformed)
    );
    bytes[7] = b'h';
    bytes[9] = 2;
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, recover),
        Err(Error::Malformed)
    );
}

#[test]
fn appendable_tuple_tails_preserve_inner_and_outer_positions() {
    let mut scratch = [0; 16];
    let mut bytes = Vec::new();
    let current = TupleTail {
        extra: Some(TextRef::new("hi").unwrap()),
        marker: Some(()),
    };
    encode_projected_into(&mut bytes, Encoding::Payload, LIMITS, |ctx| {
        let tail = ctx.nested(|tuple| current.encode_extensions(tuple, &mut scratch))?;
        Ok(Projected::empty((9_u8, CurrentTuple(257, tail), true)))
    })
    .unwrap();
    assert_eq!(bytes, [9, 0x81, 2, 7, 2, 3, 2, b'h', b'i', 3, 0, 1, 0]);

    let decode_previous = |root: Projected<'_, (u8, PreviousTuple<'_>, bool)>,
                           ctx: &ReadContext<'_>| {
        let (before, tuple, after) = project_core(root, ctx)?;
        ctx.nested(|nested| OldFields::decode_extensions(tuple.1, nested))?;
        Ok((before, tuple.0, after))
    };
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, decode_previous),
        Ok((9, 257, true))
    );
    type TupleWithTail<'a> = (u8, u32, Option<TextRef<'a, 16>>, Option<()>, bool);
    fn decode_current<'a>(
        root: Projected<'a, (u8, CurrentTuple<'a>, bool)>,
        ctx: &ReadContext<'_>,
    ) -> Result<TupleWithTail<'a>, Error> {
        let (before, tuple, after) = project_core(root, ctx)?;
        let tail = ctx.nested(|nested| TupleTail::decode_extensions(tuple.1, nested))?;
        Ok((before, tuple.0, tail.extra, tail.marker, after))
    }
    let current_read = decode_projected(&bytes, Encoding::Payload, LIMITS, decode_current).unwrap();
    assert_eq!(
        current_read,
        (9, 257, Some(TextRef::new("hi").unwrap()), Some(()), true)
    );
    assert_eq!(
        current_read.2.unwrap().as_str().as_ptr(),
        bytes[7..].as_ptr()
    );

    encode_projected_into(&mut bytes, Encoding::Payload, LIMITS, |ctx| {
        let tail = ctx.nested(|tuple| OldFields {}.encode_extensions(tuple, &mut scratch))?;
        Ok(Projected::empty((9_u8, PreviousTuple(257, tail), true)))
    })
    .unwrap();
    assert_eq!(bytes, [9, 0x81, 2, 0, 1, 0]);
    assert_eq!(
        decode_projected(&bytes, Encoding::Payload, LIMITS, decode_current),
        Ok((9, 257, None, None, true))
    );
}

#[test]
#[cfg(feature = "typescript")]
fn tuple_schema_describes_only_wire_positions() {
    let graph = ValidatedRegistry::from_type::<(
        PreviousTuple<'static>,
        CurrentTuple<'static>,
        (u32, bool),
    )>("TupleRevisions", Default::default())
    .unwrap();
    let metadata: ic_wire::typescript::__private::Value =
        serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    let items = metadata["TupleRevisions"]["items"].as_array().unwrap();
    assert_eq!(items[0]["k"], "extension-tuple");
    assert_eq!(items[0]["items"].as_array().unwrap().len(), 1);
    assert_eq!(items[0]["extensions"].as_array().unwrap().len(), 0);
    assert_eq!(items[1]["k"], "extension-tuple");
    assert_eq!(items[1]["items"].as_array().unwrap().len(), 1);
    let tails = items[1]["extensions"].as_array().unwrap();
    assert_eq!(tails.len(), 2);
    assert_eq!(tails[0]["id"], 2);
    assert_eq!(tails[0]["required"], false);
    assert_eq!(tails[0]["value"]["k"], "text");
    assert_eq!(tails[1]["id"], 3);
    assert_eq!(tails[1]["required"], false);
    assert_eq!(items[2]["k"], "tuple");
}
