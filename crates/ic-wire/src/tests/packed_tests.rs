//! Literal packed-field representation and admission contracts.

use crate::{
    Bytes, BytesRef, Direction, Encoding, Error, LimitKind, Limits, List, Packed,
    decode::decode,
    encode::{encode, encode_into, encode_to_slice},
};
use serde::{Deserialize, Serialize};
use std::{vec, vec::Vec};

fn limits() -> Limits {
    Limits {
        bytes: 128,
        allocation: 32_768,
        nodes: 512,
        depth: 32,
    }
}

#[test]
fn one_bound_contract_covers_every_packed_word() {
    macro_rules! check {
        ($ty:ty, $value:expr) => {{
            let one = Packed::<$ty, 1>::new(vec![$value]).unwrap();
            assert_eq!(
                Packed::<$ty, 1>::new(vec![$value; 2]),
                Err(Error::Limit(LimitKind::Field))
            );
            let bytes = encode(&one, Encoding::Payload, limits()).unwrap();
            assert_eq!(usize::from(bytes[0]), size_of::<$ty>());
            assert_eq!(
                decode::<Packed<$ty, 1>>(&bytes, Encoding::Payload, limits()).unwrap(),
                one
            );
            let two = Packed::<$ty>::new(vec![$value; 2]).unwrap();
            let bytes = encode(&two, Encoding::Payload, limits()).unwrap();
            assert_eq!(
                decode::<Packed<$ty, 1>>(&bytes, Encoding::Payload, limits()),
                Err(Error::Malformed)
            );
            assert!(
                decode::<Packed<$ty, 0>>(&[0], Encoding::Payload, limits())
                    .unwrap()
                    .as_slice()
                    .is_empty()
            );
        }};
    }
    check!(i8, -1);
    check!(i16, -1);
    check!(u16, 1);
    check!(i32, -1);
    check!(u32, 1);
    check!(i64, -1);
    check!(u64, 1);
    check!([u8; 16], [1; 16]);
    check!([u8; 32], [1; 32]);
}

#[test]
fn packed_vectors_are_byte_counted_and_little_endian() {
    for (words, literal) in [
        (vec![], vec![0]),
        (vec![0], vec![2, 0, 0]),
        (vec![1, -1], vec![4, 1, 0, 255, 255]),
        (vec![i16::MIN, i16::MAX], vec![4, 0, 128, 255, 127]),
    ] {
        let packed = Packed::<i16, 2>::new(words.clone()).unwrap();
        assert_eq!(
            encode(&packed, Encoding::Payload, limits()).unwrap(),
            literal
        );
        assert_eq!(
            decode::<Packed<i16, 2>>(&literal, Encoding::Payload, limits())
                .unwrap()
                .into_vec(),
            words
        );
    }
    assert_eq!(
        encode(&vec![1_i16, -1], Encoding::Payload, limits()).unwrap(),
        [2, 2, 1]
    );
    assert_eq!(
        encode(
            &List::<i16, 2>::new(vec![1, -1]).unwrap(),
            Encoding::Payload,
            limits()
        )
        .unwrap(),
        [2, 2, 1]
    );
}

#[test]
fn bounds_widths_and_error_precedence_are_preserved() {
    assert_eq!(
        Packed::<i16, 0>::new(vec![1]),
        Err(Error::Limit(LimitKind::Field))
    );
    assert!(Packed::<i16, 0>::new(vec![]).is_ok());
    for bytes in [&[1, 0][..], &[3, 0, 0, 0], &[4, 0, 0], &[4, 0, 0, 0, 0]] {
        assert_eq!(
            decode::<Packed<i16, 1>>(bytes, Encoding::Payload, limits()),
            Err(Error::Malformed)
        );
    }
    assert_eq!(
        decode::<Packed<i16, 0>>(&[2, 0, 0], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<Packed<i16, 1>>(
            &[1, 0],
            Encoding::Payload,
            Limits {
                nodes: 0,
                ..limits()
            }
        ),
        Err(Error::Limit(LimitKind::Nodes))
    );
    assert_eq!(
        decode::<Packed<i16, 1>>(&[2, 0, 0, 99], Encoding::Payload, limits()),
        Err(Error::Trailing)
    );
}

#[test]
fn packed_admission_counts_one_bulk_span_not_word_visits() {
    let packed = Packed::<i16, 2>::new(vec![1, -1]).unwrap();
    let encode_limits = Limits {
        bytes: 5,
        allocation: 5 + 16 * size_of::<Packed<i16, 2>>() + 32,
        nodes: 4,
        depth: 3,
    };
    assert_eq!(
        encode(&packed, Encoding::Payload, encode_limits).unwrap(),
        [4, 1, 0, 255, 255]
    );
    assert_eq!(
        encode(
            &packed,
            Encoding::Payload,
            Limits {
                allocation: encode_limits.allocation - 1,
                ..encode_limits
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
    let decode_limits = Limits {
        bytes: 5,
        allocation: 5 + 8 * (size_of::<Vec<i16>>() + 4),
        nodes: 1,
        depth: 1,
    };
    assert_eq!(
        decode::<Packed<i16, 2>>(&[4, 1, 0, 255, 255], Encoding::Payload, decode_limits).unwrap(),
        packed
    );
    assert_eq!(
        decode::<Packed<i16, 2>>(
            &[4, 1, 0, 255, 255],
            Encoding::Payload,
            Limits {
                allocation: decode_limits.allocation - 1,
                ..decode_limits
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

#[test]
fn constructors_preserve_storage_and_decoding_owns_unaligned_input() {
    let words = vec![1, 2];
    let pointer = words.as_ptr();
    let packed = Packed::<i16, 2>::new(words).unwrap();
    assert_eq!(packed.as_slice().as_ptr(), pointer);
    let recovered = packed.into_vec();
    assert_eq!(recovered.as_ptr(), pointer);
    let mut backing = [99, 4, 1, 0, 255, 255, 88];
    let decoded: Packed<i16, 2> = decode(&backing[1..6], Encoding::Payload, limits()).unwrap();
    backing.fill(0);
    assert_eq!(decoded.as_slice(), &[1, -1]);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Choice {
    Ready,
    Value(u16),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    number: u8,
    choice: Choice,
    optional: Option<u8>,
    bytes: Bytes<3>,
    packed: Packed<i16, 2>,
    ordinary: List<i16, 2>,
}

#[derive(Deserialize)]
struct Projection<'a> {
    number: u8,
    choice: Choice,
    optional: Option<u8>,
    #[serde(borrow)]
    bytes: BytesRef<'a, 3>,
    packed: Packed<i16, 2>,
    ordinary: List<i16, 2>,
}

#[test]
fn representative_empty_small_and_large_value_controls() {
    for (words, literal) in [
        (vec![], vec![0, 0, 0, 0, 0, 0]),
        (vec![0], vec![0, 0, 0, 0, 2, 0, 0, 1, 0]),
        (vec![32767], vec![0, 0, 0, 0, 2, 255, 127, 1, 254, 255, 3]),
    ] {
        let record = Record {
            number: 0,
            choice: Choice::Ready,
            optional: None,
            bytes: Bytes::new(vec![]).unwrap(),
            packed: Packed::new(words.clone()).unwrap(),
            ordinary: List::new(words).unwrap(),
        };
        assert_eq!(
            encode(&record, Encoding::Payload, limits()).unwrap(),
            literal
        );
        assert_eq!(
            decode::<Record>(&literal, Encoding::Payload, limits()).unwrap(),
            record
        );
    }
}

#[test]
fn representative_record_matches_literal_payload_and_frames() {
    let record = Record {
        number: 7,
        choice: Choice::Value(128),
        optional: Some(9),
        bytes: Bytes::new(vec![0xaa, 0xbb]).unwrap(),
        packed: Packed::new(vec![1, -1]).unwrap(),
        ordinary: List::new(vec![1, -1]).unwrap(),
    };
    let payload = [
        7, 1, 0x80, 1, 1, 9, 2, 0xaa, 0xbb, 4, 1, 0, 255, 255, 2, 2, 1,
    ];
    for encoding in [
        Encoding::Payload,
        Encoding::Frame {
            method: 7,
            direction: Direction::Request,
        },
        Encoding::Frame {
            method: 7,
            direction: Direction::Response,
        },
    ] {
        let mut expected = match encoding {
            Encoding::Payload => vec![],
            Encoding::Frame {
                direction: Direction::Request,
                ..
            } => vec![b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 0],
            Encoding::Frame {
                direction: Direction::Response,
                ..
            } => vec![b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 1],
        };
        let prefix = expected.len();
        expected.extend_from_slice(&payload);
        assert_eq!(encode(&record, encoding, limits()).unwrap(), expected);
        let mut reused = vec![255];
        assert_eq!(
            encode_into(&record, encoding, limits(), &mut reused).unwrap(),
            expected
        );
        let mut storage = [255; 128];
        assert_eq!(
            encode_to_slice(&record, encoding, limits(), &mut storage).unwrap(),
            expected
        );
        assert_eq!(
            decode::<Record>(&expected, encoding, limits()).unwrap(),
            record
        );
        let view: Projection<'_> = decode(&expected, encoding, limits()).unwrap();
        assert_eq!(view.number, record.number);
        assert_eq!(view.choice, record.choice);
        assert_eq!(view.optional, record.optional);
        assert_eq!(view.bytes.as_slice(), record.bytes.as_slice());
        assert_eq!(
            view.bytes.as_slice().as_ptr(),
            expected[prefix + 7..].as_ptr()
        );
        assert_eq!(view.packed, record.packed);
        assert_eq!(view.ordinary, record.ordinary);
    }
}
