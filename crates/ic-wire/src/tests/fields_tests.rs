//! Literal field contracts and bounded visitor/custody checks.

use crate::{
    Bytes, BytesRef, Direction, Encoding, Error, LimitKind, Limits, List, Text, TextRef,
    decode::decode,
    encode::{encode, encode_into, encode_to_slice},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{cell::Cell, thread_local, vec, vec::Vec};

fn limits() -> Limits {
    Limits {
        bytes: 128,
        allocation: 32_768,
        nodes: 512,
        depth: 32,
    }
}

#[test]
fn constructors_admit_exact_bounds_without_copying() {
    let bytes = vec![1, 2];
    let pointer = bytes.as_ptr();
    let bytes = Bytes::<2>::new(bytes).unwrap();
    assert_eq!(bytes.as_slice().as_ptr(), pointer);
    assert_eq!(bytes.as_borrowed().as_slice().as_ptr(), pointer);
    let recovered = bytes.into_vec();
    assert_eq!(recovered.as_ptr(), pointer);
    assert_eq!(
        Bytes::<1>::new(vec![1, 2]),
        Err(Error::Limit(LimitKind::Field))
    );
    assert_eq!(
        List::<u8, 1>::new(vec![1, 2]),
        Err(Error::Limit(LimitKind::Field))
    );
    assert_eq!(
        Text::<1>::new("é".into()),
        Err(Error::Limit(LimitKind::Field))
    );
    let text = Text::<2>::new("é".into()).unwrap();
    assert_eq!(text.as_str(), "é");
    assert_eq!(text.as_borrowed().as_str().as_ptr(), text.as_str().as_ptr());
    assert_eq!(text.into_string(), "é");
    assert!(Bytes::<0>::new(vec![]).is_ok());
    assert!(TextRef::<0>::new("").is_ok());
    assert_eq!(
        BytesRef::<0>::new(&[1]),
        Err(Error::Limit(LimitKind::Field))
    );
    assert_eq!(TextRef::<1>::new("é"), Err(Error::Limit(LimitKind::Field)));
}

#[test]
fn literal_fields_and_ordinary_vectors_keep_their_representations() {
    assert_eq!(
        encode(
            &Bytes::<2>::new(vec![9, 10]).unwrap(),
            Encoding::Payload,
            limits()
        )
        .unwrap(),
        [2, 9, 10]
    );
    assert_eq!(
        encode(
            &Text::<2>::new("é".into()).unwrap(),
            Encoding::Payload,
            limits()
        )
        .unwrap(),
        [2, 0xc3, 0xa9]
    );
    let list = List::<i16, 2>::new(vec![0, 32767]).unwrap();
    let literal = [2, 0, 0xfe, 0xff, 3];
    assert_eq!(encode(&list, Encoding::Payload, limits()).unwrap(), literal);
    assert_eq!(
        encode(&vec![0_i16, 32767], Encoding::Payload, limits()).unwrap(),
        literal
    );
    assert_eq!(
        decode::<List<i16, 2>>(&literal, Encoding::Payload, limits())
            .unwrap()
            .into_vec(),
        vec![0, 32767]
    );
    assert_eq!(
        decode::<Bytes<2>>(&[2, 9, 10], Encoding::Payload, limits())
            .unwrap()
            .into_vec(),
        [9, 10]
    );
    assert_eq!(
        decode::<Text<2>>(&[2, 0xc3, 0xa9], Encoding::Payload, limits())
            .unwrap()
            .as_str(),
        "é"
    );
}

#[test]
fn bulk_and_list_admission_remain_distinct_despite_equal_bytes() {
    let bulk = Bytes::<2>::new(vec![9, 10]).unwrap();
    let exact = Limits {
        bytes: 3,
        allocation: 3 + 16 * size_of::<Bytes<2>>() + 16,
        nodes: 4,
        depth: 3,
    };
    assert_eq!(encode(&bulk, Encoding::Payload, exact).unwrap(), [2, 9, 10]);
    assert_eq!(
        encode(&vec![9_u8, 10], Encoding::Payload, exact),
        Err(Error::Limit(LimitKind::Depth))
    );
    assert_eq!(
        encode(
            &bulk,
            Encoding::Payload,
            Limits {
                allocation: exact.allocation - 1,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
    let exact = Limits {
        bytes: 3,
        allocation: 3 + 8 * (size_of::<Vec<u8>>() + 2),
        nodes: 1,
        depth: 1,
    };
    assert_eq!(
        decode::<Bytes<2>>(&[2, 9, 10], Encoding::Payload, exact).unwrap(),
        bulk
    );
    assert_eq!(
        decode::<Vec<u8>>(&[2, 9, 10], Encoding::Payload, exact),
        Err(Error::Limit(LimitKind::Depth))
    );
    assert_eq!(
        decode::<Bytes<2>>(
            &[2, 9, 10],
            Encoding::Payload,
            Limits {
                allocation: exact.allocation - 1,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
    let list = List::<u8, 2>::new(vec![9, 10]).unwrap();
    let exact = Limits {
        nodes: 5,
        depth: 2,
        allocation: 3 + 8 * (size_of::<Vec<u8>>() + 4),
        ..exact
    };
    assert_eq!(
        decode::<List<u8, 2>>(&[2, 9, 10], Encoding::Payload, exact).unwrap(),
        list
    );
    assert_eq!(
        decode::<List<u8, 2>>(&[2, 9, 10], Encoding::Payload, Limits { nodes: 4, ..exact }),
        Err(Error::Limit(LimitKind::Nodes))
    );
}

#[test]
fn field_refusal_is_malformed_and_budget_refusal_takes_precedence() {
    assert_eq!(
        decode::<Bytes<1>>(&[2, 9, 10], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<Text<1>>(&[2, 0xc3, 0xa9], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<Text<2>>(&[1, 255], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<List<(), 1>>(&[2], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<List<(), 1>>(
            &[2],
            Encoding::Payload,
            Limits {
                nodes: 3,
                ..limits()
            }
        ),
        Err(Error::Limit(LimitKind::Nodes))
    );
    assert_eq!(
        decode::<Bytes<1>>(
            &[2, 9, 10],
            Encoding::Payload,
            Limits {
                allocation: 0,
                ..limits()
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
    assert_eq!(
        decode::<Bytes<2>>(&[3, 9, 10], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<List<(), 0>>(&[0], Encoding::Payload, limits())
            .unwrap()
            .as_slice(),
        &[]
    );
}

#[test]
fn borrowed_fields_point_into_input_and_refuse_transient_visits() {
    let input = [2, b'a', b'b'];
    let bytes: BytesRef<'_, 2> = decode(&input, Encoding::Payload, limits()).unwrap();
    let text: TextRef<'_, 2> = decode(&input, Encoding::Payload, limits()).unwrap();
    assert_eq!(bytes.as_slice().as_ptr(), input[1..].as_ptr());
    assert_eq!(text.as_str().as_ptr(), input[1..].as_ptr());
    assert_eq!(encode(&bytes, Encoding::Payload, limits()).unwrap(), input);
    assert_eq!(encode(&text, Encoding::Payload, limits()).unwrap(), input);
    let transient = serde::de::value::BytesDeserializer::<serde::de::value::Error>::new(b"ab");
    assert!(BytesRef::<2>::deserialize(transient).is_err());
    let transient = serde::de::value::StrDeserializer::<serde::de::value::Error>::new("ab");
    assert!(TextRef::<2>::deserialize(transient).is_err());
    assert_eq!(
        decode::<BytesRef<'_, 1>>(&input, Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<TextRef<'_, 2>>(&[1, 255], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
}

thread_local! { static VISITS: Cell<usize> = const { Cell::new(0) }; }

#[derive(Debug)]
struct Counted;
impl<'de> Deserialize<'de> for Counted {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        VISITS.with(|count| count.set(count.get() + 1));
        <()>::deserialize(d)?;
        Ok(Self)
    }
}

#[test]
fn excess_list_element_is_not_deserialized() {
    VISITS.with(|count| count.set(0));
    assert!(matches!(
        decode::<List<Counted, 1>>(&[2], Encoding::Payload, limits()),
        Err(Error::Malformed)
    ));
    VISITS.with(|count| assert_eq!(count.get(), 1));
    VISITS.with(|count| count.set(0));
    assert!(matches!(
        decode::<List<Counted, 0>>(&[1], Encoding::Payload, limits()),
        Err(Error::Malformed)
    ));
    VISITS.with(|count| assert_eq!(count.get(), 0));
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
    text: Text<4>,
    values: List<u16, 2>,
}

#[test]
fn bounded_record_matches_literal_payload_and_both_frame_directions() {
    let record = Record {
        number: 7,
        choice: Choice::Value(128),
        optional: Some(9),
        bytes: Bytes::new(vec![0xaa, 0xbb]).unwrap(),
        text: Text::new("é".into()).unwrap(),
        values: List::new(vec![1, 128]).unwrap(),
    };
    let payload = [
        7, 1, 0x80, 1, 1, 9, 2, 0xaa, 0xbb, 2, 0xc3, 0xa9, 2, 1, 0x80, 1,
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
    }
}
