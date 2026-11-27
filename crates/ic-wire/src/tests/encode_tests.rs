//! Independent byte vectors and encoding custody/admission boundaries.

use crate::{
    Direction, Encoding, Error, LimitKind, Limits,
    decode::decode,
    encode::{encode, encode_into, encode_to_slice, encoded_len},
};
use serde::{Serialize, Serializer};
use std::{cell::Cell, collections::BTreeMap, vec, vec::Vec};

fn limits() -> Limits {
    Limits {
        bytes: 128,
        allocation: 16_384,
        nodes: 256,
        depth: 32,
    }
}

struct WideTag(bool);

impl Serialize for WideTag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0 {
            serializer.serialize_newtype_variant("Wide", 128, "Value", &7_u8)
        } else {
            serializer.serialize_unit_variant("Wide", 128, "Unit")
        }
    }
}

struct Bulk<'a>(&'a [u8]);

impl Serialize for Bulk<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

#[test]
fn wide_variant_tags_and_bulk_bytes_delegate_without_extra_visits() {
    destinations(&WideTag(false), &[0x80, 1], Encoding::Payload);
    destinations(&WideTag(true), &[0x80, 1, 7], Encoding::Payload);
    let exact = Limits {
        bytes: 3,
        allocation: 3 + 16 * size_of::<WideTag>() + 16,
        nodes: 8,
        depth: 4,
    };
    assert_eq!(encoded_len(&WideTag(true), Encoding::Payload, exact), Ok(3));
    assert_eq!(
        encoded_len(
            &WideTag(true),
            Encoding::Payload,
            Limits { nodes: 7, ..exact }
        ),
        Err(Error::Limit(LimitKind::Nodes))
    );
    let bulk = Bulk(&[9, 10]);
    destinations(&bulk, &[2, 9, 10], Encoding::Payload);
    let exact = Limits {
        bytes: 3,
        allocation: 3 + 16 * size_of::<Bulk<'_>>() + 16,
        nodes: 4,
        depth: 3,
    };
    assert_eq!(encoded_len(&bulk, Encoding::Payload, exact), Ok(3));
    assert_eq!(
        encoded_len(
            &bulk,
            Encoding::Payload,
            Limits {
                allocation: exact.allocation - 1,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

fn destinations<T: Serialize + ?Sized>(value: &T, expected: &[u8], encoding: Encoding) {
    assert_eq!(encoded_len(value, encoding, limits()), Ok(expected.len()));
    assert_eq!(encode(value, encoding, limits()).unwrap(), expected);
    let mut reused = Vec::with_capacity(128);
    reused.extend_from_slice(&[99; 16]);
    let pointer = reused.as_ptr();
    assert_eq!(
        encode_into(value, encoding, limits(), &mut reused).unwrap(),
        expected
    );
    assert_eq!(reused.as_ptr(), pointer);
    let mut storage = [0xa5; 128];
    assert_eq!(
        encode_to_slice(value, encoding, limits(), &mut storage).unwrap(),
        expected
    );
    assert!(storage[expected.len()..].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn primitive_supplier_vectors_match_in_every_destination() {
    destinations(&128_u16, &[0x80, 1], Encoding::Payload);
    destinations(&u32::MAX, &[255, 255, 255, 255, 15], Encoding::Payload);
    destinations(
        &u64::MAX,
        &[255, 255, 255, 255, 255, 255, 255, 255, 255, 1],
        Encoding::Payload,
    );
    destinations(
        &(-64_i16, 64_i32, -65_i64, -1_i8),
        &[0x7f, 0x80, 1, 0x81, 1, 255],
        Encoding::Payload,
    );
    destinations(
        &(false, true, (), 'é', 1_f32, -0_f64),
        &[
            0, 1, 2, 0xc3, 0xa9, 0, 0, 0x80, 0x3f, 0, 0, 0, 0, 0, 0, 0, 0x80,
        ],
        Encoding::Payload,
    );
    destinations("abc", &[3, b'a', b'b', b'c'], Encoding::Payload);
    destinations(
        &BTreeMap::from([(2_u8, 3_u8)]),
        &[1, 2, 3],
        Encoding::Payload,
    );
}

#[derive(Serialize)]
enum Choice {
    Unit,
    Newtype(u16),
    Tuple(u8, bool),
    Record { value: u8 },
}

#[derive(Serialize)]
struct Record {
    value: u8,
    choice: Choice,
    optional: Option<u8>,
    list: Vec<u8>,
}

#[test]
fn compound_and_frame_vectors_match_without_specialized_tag_writers() {
    destinations(&Choice::Unit, &[0], Encoding::Payload);
    destinations(&Choice::Newtype(128), &[1, 0x80, 1], Encoding::Payload);
    destinations(&Choice::Tuple(7, true), &[2, 7, 1], Encoding::Payload);
    destinations(&Choice::Record { value: 9 }, &[3, 9], Encoding::Payload);
    let record = Record {
        value: 7,
        choice: Choice::Unit,
        optional: Some(8),
        list: vec![9, 10],
    };
    destinations(&record, &[7, 0, 1, 8, 2, 9, 10], Encoding::Payload);
    destinations(
        &(7_u8, Some(128_u16), vec![9_u8, 10]),
        &[
            b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 0, 7, 1, 0x80, 1, 2, 9, 10,
        ],
        Encoding::Frame {
            method: 7,
            direction: Direction::Request,
        },
    );
    destinations(
        &42_u8,
        &[b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 1, 42],
        Encoding::Frame {
            method: 7,
            direction: Direction::Response,
        },
    );
    let encoding = Encoding::Frame {
        method: 7,
        direction: Direction::Request,
    };
    let bytes = encode(&(7_u8, Some(128_u16), vec![9_u8, 10]), encoding, limits()).unwrap();
    assert_eq!(
        decode::<(u8, Option<u16>, Vec<u8>)>(&bytes, encoding, limits()).unwrap(),
        (7, Some(128), vec![9, 10])
    );
}

#[test]
fn exact_encoding_allowances_include_four_visits_and_two_value_charges() {
    let exact = Limits {
        bytes: 2,
        allocation: 34,
        nodes: 4,
        depth: 3,
    };
    assert_eq!(
        encode(&128_u16, Encoding::Payload, exact).unwrap(),
        [0x80, 1]
    );
    for (insufficient, reason) in [
        Limits { bytes: 1, ..exact },
        Limits {
            allocation: 33,
            ..exact
        },
        Limits { nodes: 3, ..exact },
        Limits { depth: 2, ..exact },
        Limits { bytes: 3, ..exact },
    ]
    .into_iter()
    .zip([
        LimitKind::Bytes,
        LimitKind::Allocation,
        LimitKind::Nodes,
        LimitKind::Depth,
        LimitKind::Allocation,
    ]) {
        let mut reused = vec![42];
        assert_eq!(
            encode_into(&128_u16, Encoding::Payload, insufficient, &mut reused),
            Err(Error::Limit(reason))
        );
        assert!(reused.is_empty());
        let mut storage = [0xa5; 8];
        assert_eq!(
            encode_to_slice(&128_u16, Encoding::Payload, insufficient, &mut storage),
            Err(Error::Limit(reason))
        );
        assert_eq!(storage, [0xa5; 8]);
    }
    let frame = Encoding::Frame {
        method: 0,
        direction: Direction::Request,
    };
    assert_eq!(
        encoded_len(
            &128_u16,
            frame,
            Limits {
                bytes: 13,
                allocation: 45,
                ..exact
            }
        ),
        Ok(13)
    );
    assert_eq!(
        encoded_len(
            &(),
            frame,
            Limits {
                bytes: 10,
                ..limits()
            }
        ),
        Err(Error::Limit(LimitKind::Bytes))
    );
    assert_eq!(
        encoded_len(
            &(),
            frame,
            Limits {
                allocation: 10,
                ..limits()
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

#[test]
fn short_slice_is_untouched_including_frame_header() {
    let encoding = Encoding::Frame {
        method: 7,
        direction: Direction::Response,
    };
    let mut storage = [0xa5; 11];
    assert_eq!(
        encode_to_slice(&7_u8, encoding, limits(), &mut storage),
        Err(Error::Limit(LimitKind::Output))
    );
    assert_eq!(storage, [0xa5; 11]);
}

struct Changes {
    pass: Cell<usize>,
    fail: bool,
    shrink: bool,
}

impl Serialize for Changes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let first = self.pass.replace(self.pass.get() + 1) == 0;
        if !first && self.fail {
            return Err(serde::ser::Error::custom("second pass failed"));
        }
        serializer.serialize_u16(if first != self.shrink { 1 } else { 128 })
    }
}

#[test]
fn second_pass_growth_or_failure_clears_vec_and_cannot_overrun_slice() {
    for fail in [false, true] {
        let value = Changes {
            pass: Cell::new(0),
            fail,
            shrink: false,
        };
        let mut reused = Vec::with_capacity(128);
        reused.push(42);
        assert_eq!(
            encode_into(&value, Encoding::Payload, limits(), &mut reused),
            Err(Error::Malformed)
        );
        assert_eq!(value.pass.get(), 2);
        assert!(reused.is_empty());
        value.pass.set(0);
        let mut storage = [0xa5; 128];
        assert_eq!(
            encode_to_slice(&value, Encoding::Payload, limits(), &mut storage),
            Err(Error::Malformed)
        );
        assert!(storage[1..].iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn shorter_second_pass_returns_only_written_bytes_as_in_reference() {
    let value = Changes {
        pass: Cell::new(0),
        fail: false,
        shrink: true,
    };
    assert_eq!(encode(&value, Encoding::Payload, limits()).unwrap(), [1]);
    value.pass.set(0);
    let mut storage = [0xa5; 3];
    assert_eq!(
        encode_to_slice(&value, Encoding::Payload, limits(), &mut storage).unwrap(),
        [1]
    );
    assert_eq!(storage[1..], [0xa5; 2]);
}

#[test]
fn zero_width_sequences_and_text_keep_reference_charges() {
    let value = [(); 2];
    let exact = Limits {
        bytes: 0,
        allocation: 0,
        nodes: 12,
        depth: 4,
    };
    assert_eq!(encoded_len(&value, Encoding::Payload, exact), Ok(0));
    assert_eq!(
        encoded_len(&value, Encoding::Payload, Limits { nodes: 11, ..exact }),
        Err(Error::Limit(LimitKind::Nodes))
    );
    assert_eq!(
        encoded_len(&value, Encoding::Payload, Limits { depth: 3, ..exact }),
        Err(Error::Limit(LimitKind::Depth))
    );
    // Unsized str: 2 * three value bytes + three span bytes, eight each.
    let text = Limits {
        bytes: 4,
        allocation: 76,
        nodes: 4,
        depth: 3,
    };
    assert_eq!(encoded_len("abc", Encoding::Payload, text), Ok(4));
    assert_eq!(
        encoded_len(
            "abc",
            Encoding::Payload,
            Limits {
                allocation: 75,
                ..text
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

#[test]
fn fixed_record_allowances_cover_maximum_width_fields() {
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Reply {
        accepted: bool,
        revision: u64,
    }

    let bytes = 11;
    let limits = Limits {
        bytes,
        allocation: bytes + 16 * (size_of::<Reply>() + size_of::<bool>() + size_of::<u64>()),
        nodes: 12,
        depth: 4,
    };
    let value = Reply {
        accepted: true,
        revision: u64::MAX,
    };
    let payload = encode(&value, Encoding::Payload, limits).unwrap();
    assert_eq!(payload.len(), bytes);
    assert_eq!(decode(&payload, Encoding::Payload, limits), Ok(value));
}
