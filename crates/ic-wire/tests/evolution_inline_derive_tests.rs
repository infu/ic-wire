//! Inline sparse declarations use one stock Serde codec and one operation meter.
#![cfg(feature = "derive")]

use ic_wire::{
    Encoding, Error, LimitKind, Limits, TextRef, Wire, decode, encode, encode_into, encode_to_slice,
};

const LIMITS: Limits = Limits {
    bytes: 256,
    allocation: 8192,
    nodes: 256,
    depth: 12,
};

#[derive(Debug, PartialEq, Wire)]
struct Previous {
    sequence: u16,
}

#[derive(Debug, PartialEq, Wire)]
struct Current<'a, T: Copy, const MAX: usize>
where
    T: PartialEq,
{
    sequence: T,
    #[wire(id = 9)]
    stamp: Option<()>,
    #[wire(id = 4)]
    note: Option<TextRef<'a, MAX>>,
}

#[test]
fn inline_record_revisions_share_exact_bytes_and_borrowed_fields() {
    let old = Previous { sequence: 257 };
    let current = Current::<u16, 16> {
        sequence: 257,
        stamp: Some(()),
        note: Some(TextRef::new("hi").unwrap()),
    };
    let payload = Encoding::Payload;
    let mut output = encode(&old, payload, LIMITS).unwrap();
    assert_eq!(output, [0x81, 2, 0]);
    assert_eq!(
        decode::<Current<'_, u16, 16>>(&output, payload, LIMITS),
        Ok(Current {
            sequence: 257,
            note: None,
            stamp: None
        })
    );
    encode_into(&current, payload, LIMITS, &mut output).unwrap();
    assert_eq!(output, [0x81, 2, 7, 4, 3, 2, b'h', b'i', 9, 0]);
    assert_eq!(decode::<Previous>(&output, payload, LIMITS), Ok(old));
    let decoded = decode::<Current<'_, u16, 16>>(&output, payload, LIMITS).unwrap();
    assert_eq!(decoded, current);
    assert_eq!(
        decoded.note.unwrap().as_str().as_ptr(),
        output[6..].as_ptr()
    );

    let mut malformed = output.clone();
    malformed[8] = 4;
    assert_eq!(
        decode::<Previous>(&malformed, payload, LIMITS),
        Err(Error::Malformed)
    );
    malformed[8] = 9;
    malformed[6] = 0xff;
    assert_eq!(
        decode::<Current<'_, u16, 16>>(&malformed, payload, LIMITS),
        Err(Error::Malformed)
    );
    let too_shallow = Limits { depth: 2, ..LIMITS };
    assert_eq!(
        decode::<Previous>(&output, payload, too_shallow),
        Err(Error::Limit(LimitKind::Depth))
    );
    assert_eq!(
        encode_into(&current, payload, too_shallow, &mut output),
        Err(Error::Limit(LimitKind::Depth))
    );
    assert!(output.is_empty());
}

#[derive(Debug, PartialEq, Wire)]
struct PreviousTuple(u8);

#[derive(Debug, PartialEq, Wire, serde::Serialize, serde::Deserialize)]
struct CurrentTuple(u8, #[wire(id = 2)] Option<bool>);

#[test]
fn inline_tuple_does_not_change_its_separately_derived_serde_layout() {
    let latest = CurrentTuple(17, Some(true));
    let payload = Encoding::Payload;
    let mut output = encode(&latest, payload, LIMITS).unwrap();
    assert_eq!(output, [17, 3, 2, 1, 1]);
    assert_eq!(
        decode::<PreviousTuple>(&output, payload, LIMITS),
        Ok(PreviousTuple(17))
    );
    encode_into(&PreviousTuple(17), payload, LIMITS, &mut output).unwrap();
    assert_eq!(output, [17, 0]);
    assert_eq!(
        decode::<CurrentTuple>(&output, payload, LIMITS),
        Ok(CurrentTuple(17, None))
    );
    assert_eq!(postcard::to_allocvec(&latest).unwrap(), [17, 1, 1]);

    let mut storage = [0xff; 8];
    assert_eq!(
        encode_to_slice(&latest, payload, LIMITS, &mut storage),
        Ok([17, 3, 2, 1, 1].as_slice())
    );
    assert_eq!(storage[5..], [0xff; 3]);
    assert_eq!(
        encode_to_slice(&latest, payload, LIMITS, &mut storage[..4]),
        Err(Error::Limit(LimitKind::Output))
    );
}

#[derive(Debug, PartialEq, Wire)]
struct Wide(
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
);

#[test]
fn inline_tuple_core_exceeds_serde_tuple_trait_arity() {
    let wide = Wide(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
    let limits = Limits {
        allocation: 16_384,
        ..LIMITS
    };
    let output = encode(&wide, Encoding::Payload, limits).unwrap();
    assert_eq!(
        output,
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 0
        ]
    );
    assert_eq!(decode::<Wide>(&output, Encoding::Payload, limits), Ok(wide));
}

#[derive(Debug, PartialEq, Wire)]
struct Empty;

#[test]
fn inline_unit_sends_and_validates_an_extension_block() {
    let payload = Encoding::Payload;
    let bytes = encode(&Empty, payload, LIMITS).unwrap();
    assert_eq!(bytes, [0]);
    assert_eq!(decode::<Empty>(&bytes, payload, LIMITS), Ok(Empty));
    assert_eq!(decode::<Empty>(&[], payload, LIMITS), Err(Error::Malformed));
    assert_eq!(decode::<Empty>(&[2, 7, 0], payload, LIMITS), Ok(Empty));
    assert_eq!(
        decode::<Empty>(&[2, 0, 0], payload, LIMITS),
        Err(Error::Malformed)
    );
}

mod public_record {
    use super::*;

    #[derive(Debug, PartialEq, Wire)]
    struct PrivateRow<'a, T> {
        id: T,
        #[wire(id = 1)]
        note: Option<TextRef<'a, 5>>,
    }

    /// A public DTO can keep its nested projection's type private.
    #[derive(Debug, PartialEq, Wire)]
    pub struct Packet<'a, T> {
        row: PrivateRow<'a, T>,
        checksum: u16,
    }

    #[test]
    fn public_inline_record_keeps_private_nested_fields() {
        let packet = Packet {
            row: PrivateRow {
                id: 7_u8,
                note: Some(TextRef::new("x").unwrap()),
            },
            checksum: 8,
        };
        let output = encode(&packet, Encoding::Payload, LIMITS).unwrap();
        assert_eq!(output, [7, 4, 1, 2, 1, b'x', 8, 0]);
        assert_eq!(
            decode::<Packet<'_, u8>>(&output, Encoding::Payload, LIMITS),
            Ok(packet)
        );
    }
}

#[test]
fn codec_only_wire_accepts_fixed_struct_and_closed_enum_leaves() {
    #[derive(Debug, PartialEq, Wire, serde::Serialize, serde::Deserialize)]
    #[wire(fixed)]
    struct Key(u8);

    #[derive(Debug, PartialEq, Wire, serde::Serialize, serde::Deserialize)]
    enum Mode {
        Set(u8),
    }

    #[derive(Debug, PartialEq, Wire)]
    struct Root {
        key: Key,
        mode: Mode,
    }

    let value = Root {
        key: Key(9),
        mode: Mode::Set(42),
    };
    let output = encode(&value, Encoding::Payload, LIMITS).unwrap();
    assert_eq!(output, [9, 0, 42, 0]);
    assert_eq!(
        decode::<Root>(&output, Encoding::Payload, LIMITS),
        Ok(value)
    );
}
