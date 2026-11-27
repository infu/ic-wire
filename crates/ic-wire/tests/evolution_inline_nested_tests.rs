//! Recursive projection through auto-derived arrays and sequences.
#![cfg(feature = "derive")]

use ic_wire::{
    BytesRef, Encoding, Error, LimitKind, Limits, TextRef, Wire, decode, encode, encode_into,
    encode_to_slice, evolution::Method,
};

const LIMITS: Limits = Limits {
    bytes: 128,
    allocation: 16_384,
    nodes: 1024,
    depth: 24,
};

#[derive(Debug, PartialEq, Eq, Wire)]
struct ItemV1<'a> {
    name: TextRef<'a, 16>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
struct ItemV2<'a> {
    name: TextRef<'a, 16>,
    #[wire(id = 1)]
    detail: Option<TextRef<'a, 16>>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
struct Collection<T> {
    fixed_count: [T; 2],
    spare: Vec<T>,
    witness: u16,
}

fn method_v1<'a>() -> Method<Collection<ItemV1<'a>>, Collection<ItemV1<'a>>> {
    Method::new("collections", LIMITS, LIMITS)
}

fn method_v2<'a>() -> Method<Collection<ItemV2<'a>>, Collection<ItemV2<'a>>> {
    Method::new("collections", LIMITS, LIMITS)
}

#[test]
fn nested_arrays_and_vecs_project_known_values_and_reuse_input_borrows() {
    let first = ItemV2 {
        name: TextRef::new("a").unwrap(),
        detail: None,
    };
    let latest = ItemV2 {
        name: TextRef::new("b").unwrap(),
        detail: Some(TextRef::new("detail Δ").unwrap()),
    };
    let value = Collection {
        fixed_count: [first, latest],
        spare: vec![ItemV2 {
            name: TextRef::new("c").unwrap(),
            detail: None,
        }],
        witness: 42,
    };
    let mut frame = Vec::new();
    method_v2().encode_reply(&value, &mut frame).unwrap();
    assert_eq!(
        &frame[11..],
        [
            1, b'a', 0, 1, b'b', 12, 1, 10, 9, b'd', b'e', b't', b'a', b'i', b'l', b' ', 0xce,
            0x94, 1, 1, b'c', 0, 42, 0,
        ]
    );

    let latest = method_v2().decode_reply(&frame).unwrap();
    assert_eq!(latest, value);
    let detail = latest.fixed_count[1].detail.unwrap().as_str();
    assert!(frame.as_ptr_range().contains(&detail.as_ptr()));

    let first = method_v1().decode_reply(&frame).unwrap();
    assert_eq!(first.witness, 42);
    assert_eq!(first.fixed_count[0].name.as_str(), "a");
    assert_eq!(first.fixed_count[1].name.as_str(), "b");
    assert_eq!(first.spare[0].name.as_str(), "c");

    let mut old_frame = Vec::new();
    method_v1().encode_reply(&first, &mut old_frame).unwrap();
    assert_eq!(
        &old_frame[11..],
        [1, b'a', 0, 1, b'b', 0, 1, 1, b'c', 0, 42, 0]
    );
    let upgraded = method_v2().decode_reply(&old_frame).unwrap();
    assert_eq!(upgraded.witness, 42);
    assert_eq!(upgraded.fixed_count[1].detail, None);
    assert_eq!(upgraded.spare[0].detail, None);
}

#[test]
fn nested_projection_obeys_the_frame_limits() {
    let value = Collection {
        fixed_count: [
            ItemV2 {
                name: TextRef::new("a").unwrap(),
                detail: None,
            },
            ItemV2 {
                name: TextRef::new("b").unwrap(),
                detail: Some(TextRef::new("detail").unwrap()),
            },
        ],
        spare: Vec::new(),
        witness: 42,
    };
    let mut frame = Vec::new();
    method_v2().encode_reply(&value, &mut frame).unwrap();
    let low_allocation = Limits {
        allocation: 256,
        ..LIMITS
    };
    let constrained = Method::<Collection<ItemV2<'_>>, Collection<ItemV2<'_>>>::new(
        "collections",
        LIMITS,
        low_allocation,
    );
    assert_eq!(
        constrained.decode_reply(&frame),
        Err(Error::Limit(LimitKind::Allocation))
    );
    let mut constrained_output = Vec::new();
    assert_eq!(
        constrained.encode_reply(&value, &mut constrained_output),
        Err(Error::Limit(LimitKind::Allocation)),
    );
    assert!(constrained_output.is_empty());

    let shallow = Limits { depth: 2, ..LIMITS };
    let method = Method::<Collection<ItemV2<'_>>, Collection<ItemV2<'_>>>::new(
        "collections",
        LIMITS,
        shallow,
    );
    assert_eq!(
        method.decode_reply(&frame),
        Err(Error::Limit(LimitKind::Depth))
    );
    assert_eq!(
        method.encode_reply(&value, &mut frame),
        Err(Error::Limit(LimitKind::Depth))
    );
    assert!(frame.is_empty());
}

#[test]
fn ordinary_encoding_stages_large_extensions_nested_inside_extensions() {
    #[derive(Debug, PartialEq, Wire)]
    struct V1 {
        sequence: u8,
    }

    #[derive(Debug, PartialEq, Wire)]
    struct V2<'a> {
        sequence: u8,
        #[wire(id = 3)]
        detail: Option<Detail<'a>>,
    }

    #[derive(Debug, PartialEq, Wire)]
    struct Detail<'a> {
        status: u8,
        #[wire(id = 1)]
        attachment: Option<BytesRef<'a, 4096>>,
    }

    const PREFIX: [u8; 14] = [
        9, 0x8b, 0x20, 3, 0x88, 0x20, 7, 0x85, 0x20, 1, 0x82, 0x20, 0x80, 0x20,
    ];
    const BYTES: usize = 4096 + PREFIX.len();
    let limits = Limits {
        bytes: BYTES,
        allocation: 256 * 1024,
        ..LIMITS
    };
    let payload = Encoding::Payload;
    let attachment = [42; 4096];
    let original = V1 { sequence: 9 };
    let revised = V2 {
        sequence: 9,
        detail: Some(Detail {
            status: 7,
            attachment: Some(BytesRef::new(&attachment).unwrap()),
        }),
    };
    let mut encoded = encode(&original, payload, limits).unwrap();
    assert_eq!(encoded, [9, 0]);
    assert_eq!(
        decode::<V2<'_>>(&encoded, payload, limits),
        Ok(V2 {
            sequence: 9,
            detail: None
        })
    );
    encode_into(&revised, payload, limits, &mut encoded).unwrap();
    assert_eq!(encoded.len(), BYTES);
    assert_eq!(encoded[..PREFIX.len()], PREFIX);
    assert_eq!(encoded[PREFIX.len()..], attachment);
    assert_eq!(decode::<V1>(&encoded, payload, limits), Ok(original));
    assert_eq!(
        decode::<V2<'_>>(&encoded, payload, limits).unwrap(),
        revised
    );

    let mut destination = vec![0xff; BYTES + 1];
    assert_eq!(
        encode_to_slice(&revised, payload, limits, &mut destination),
        Ok(encoded.as_slice())
    );
    assert_eq!(destination[BYTES], 0xff);

    let short = Limits {
        bytes: BYTES - 1,
        ..limits
    };
    assert_eq!(
        encode_into(&revised, payload, short, &mut encoded),
        Err(Error::Limit(LimitKind::Bytes))
    );
    assert!(encoded.is_empty());
}
